#[cfg(feature = "crc")]
use crc32fast::Hasher;
#[cfg(feature = "deflate")]
use flate2::{self, write::DeflateEncoder};
use std::{
	io::{self, Error, ErrorKind, Write},
	mem::replace,
};

mod date;
#[cfg(test)]
mod test;
#[cfg(feature = "tokio")]
pub mod tokio;

pub use date::DateTime;

const CENTRAL_DIRECTORY_HEADER: &[u8] = &[0x50, 0x4B, 0x01, 0x02];
const END_CENTRAL_DIRECTORY: &[u8] = &[0x50, 0x4B, 0x05, 0x06];
const LOCAL_HEADER: &[u8] = &[0x50, 0x4B, 0x03, 0x04];
const PLATFORM: &[u8] = &[0x00, 0x00];
const VERSION: &[u8] = &[0x14, 0x00];

pub enum Compression {
	#[cfg(feature = "deflate")]
	Deflate,
	None,
}

impl Compression {
	fn to_le_bytes(&self) -> [u8; 2] {
		match self {
			#[cfg(feature = "deflate")]
			Self::Deflate => [0x08, 0x00],
			Self::None => [0x00, 0x00],
		}
	}
}

struct Entry {
	pub compression: Compression,
	pub crc: u32,
	pub date_time: DateTime,
	pub name: String,
	pub position: u64,
	pub raw_size: u64,
	pub size: u64,
}

enum Writer<W: Write> {
	#[cfg(feature = "deflate")]
	Deflate(DeflateEncoder<W>),
	Raw(W),
	None,
}

pub struct Zip<W: Write> {
	#[cfg(feature = "crc")]
	crc: Hasher,
	cursor: u64,
	entries: Vec<Entry>,
	writer: Writer<W>,
}

impl<W: Write> Zip<W> {
	pub fn new(writer: W) -> Self {
		Self {
			#[cfg(feature = "crc")]
			crc: Hasher::new(),
			entries: Vec::new(),
			cursor: 0,
			writer: Writer::Raw(writer),
		}
	}

	pub fn create_entry<T: Into<String>>(
		&mut self,
		name: T,
		compression: Compression,
		date_time: DateTime,
	) -> io::Result<()> {
		let name = name.into();
		if name.len() > u16::MAX.into() {
			return Err(Error::new(ErrorKind::InvalidInput, ""));
		}
		let mut writer = self.commit_previous()?;
		writer.write_all(LOCAL_HEADER)?;
		writer.write_all(VERSION)?;
		writer.write_all(&[0b00001000, 0b00001000])?;
		writer.write_all(&compression.to_le_bytes())?;
		writer.write_all(&date_time.to_le_bytes())?;
		writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
		writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
		writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
		writer.write_all(&(name.len() as u16).to_le_bytes())?;
		writer.write_all(&[0x00, 0x00])?;
		writer.write_all(name.as_bytes())?;
		_ = replace(
			&mut self.writer,
			match compression {
				#[cfg(feature = "deflate")]
				Compression::Deflate => {
					Writer::Deflate(DeflateEncoder::new(writer, flate2::Compression::default()))
				}
				Compression::None => Writer::Raw(writer),
			},
		);
		let position = self.cursor;
		self.cursor += 30 + name.len() as u64;
		self.entries.push(Entry {
			compression,
			crc: 0,
			date_time,
			name,
			position,
			raw_size: 0,
			size: self.cursor,
		});

		Ok(())
	}

	pub fn finish(mut self) -> io::Result<()> {
		let mut writer = self.commit_previous()?;
		let position = self.cursor;
		for entry in &self.entries {
			writer.write_all(CENTRAL_DIRECTORY_HEADER)?;
			writer.write_all(PLATFORM)?;
			writer.write_all(VERSION)?;
			writer.write_all(&[0b00001000, 0b00001000])?;
			writer.write_all(&entry.compression.to_le_bytes())?;
			writer.write_all(&entry.date_time.to_le_bytes())?;
			writer.write_all(&entry.crc.to_le_bytes())?;
			writer.write_all(&(entry.size as u32).to_le_bytes())?;
			writer.write_all(&(entry.raw_size as u32).to_le_bytes())?;
			writer.write_all(&(entry.name.len() as u16).to_le_bytes())?;
			writer.write_all(&[0x00, 0x00])?;
			writer.write_all(&[0x00, 0x00])?;
			writer.write_all(&[0x00, 0x00])?;
			writer.write_all(&[0x00, 0x00])?;
			writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
			writer.write_all(&(entry.position as u32).to_le_bytes())?;
			writer.write_all(entry.name.as_bytes())?;
			self.cursor += 46 + entry.name.len() as u64;
		}
		let number_entries = self.entries.len() as u16;
		let size = (self.cursor - position) as u32;
		writer.write_all(END_CENTRAL_DIRECTORY)?;
		writer.write_all(&[0x00, 0x00])?;
		writer.write_all(&[0x00, 0x00])?;
		writer.write_all(&number_entries.to_le_bytes())?;
		writer.write_all(&number_entries.to_le_bytes())?;
		writer.write_all(&size.to_le_bytes())?;
		writer.write_all(&(position as u32).to_le_bytes())?;
		writer.write_all(&[0x00, 0x00])?;

		Ok(())
	}

	fn commit_previous(&mut self) -> io::Result<W> {
		let writer = replace(&mut self.writer, Writer::None);
		let Some(entry) = &mut self.entries.last_mut() else {
			return Ok(match writer {
				#[cfg(feature = "deflate")]
				Writer::Deflate(encoder) => encoder.finish()?,
				Writer::Raw(writer) => writer,
				Writer::None => unreachable!(),
			});
		};
		#[cfg(feature = "crc")]
		{
			entry.crc = self.crc.clone().finalize();
			self.crc.reset();
		}
		let start = entry.size;
		entry.raw_size = self.cursor - start;
		let (mut writer, size) = match writer {
			#[cfg(feature = "deflate")]
			Writer::Deflate(mut encoder) => {
				encoder.flush()?;
				let size = encoder.total_out() + 2;
				(encoder.finish()?, size)
			}
			Writer::Raw(writer) => (writer, entry.raw_size),
			Writer::None => unreachable!(),
		};
		entry.size = size;
		writer.write_all(&entry.crc.to_le_bytes())?;
		writer.write_all(&(entry.size as u32).to_le_bytes())?;
		writer.write_all(&(entry.raw_size as u32).to_le_bytes())?;
		self.cursor = start + entry.size + 12;

		Ok(writer)
	}
}

impl<W: Write> Write for Zip<W> {
	fn write(&mut self, data: &[u8]) -> io::Result<usize> {
		let size = match &mut self.writer {
			#[cfg(feature = "deflate")]
			Writer::Deflate(writer) => writer.write(data),
			Writer::Raw(writer) => writer.write(data),
			Writer::None => unreachable!(),
		}?;
		#[cfg(feature = "crc")]
		self.crc.update(data);
		self.cursor += size as u64;
		Ok(size)
	}

	fn flush(&mut self) -> io::Result<()> {
		match &mut self.writer {
			#[cfg(feature = "deflate")]
			Writer::Deflate(writer) => writer.flush(),
			Writer::Raw(writer) => writer.flush(),
			Writer::None => unreachable!(),
		}
	}
}
