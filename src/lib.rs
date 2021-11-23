#[cfg(feature = "crc")]
use crc32fast::Hasher;
use std::io::{self, Error, ErrorKind, Write};

mod date;
#[cfg(feature = "tokio")]
pub mod stream;
#[cfg(test)]
mod test;

pub use date::DateTime;

const CENTRAL_DIRECTORY_HEADER: &[u8] = &[0x50, 0x4B, 0x01, 0x02];
const END_CENTRAL_DIRECTORY: &[u8] = &[0x50, 0x4B, 0x05, 0x06];
const LOCAL_HEADER: &[u8] = &[0x50, 0x4B, 0x03, 0x04];
const PLATFORM: &[u8] = &[0x00, 0x00];
const VERSION: &[u8] = &[0x14, 0x00];

pub enum Compression {
	None,
}

impl Compression {
	fn to_le_bytes(&self) -> [u8; 2] {
		[0x00, 0x00]
	}
}

struct Entry {
	pub crc: u32,
	pub date_time: DateTime,
	pub name: String,
	pub position: u64,
	pub size: u64,
}

pub struct Writer<W: Write> {
	#[cfg(feature = "crc")]
	crc: Hasher,
	entries: Vec<Entry>,
	size: u64,
	writer: W,
}

impl<W: Write> Writer<W> {
	pub fn new(writer: W) -> Self {
		Self {
			#[cfg(feature = "crc")]
			crc: Hasher::new(),
			entries: Vec::new(),
			size: 0,
			writer,
		}
	}

	pub fn create_entry<T: Into<String>>(
		&mut self,
		name: T,
		_: Compression,
		date_time: DateTime,
	) -> io::Result<()> {
		self.write_descriptor()?;
		let name = name.into();
		if name.len() > u16::MAX.into() {
			return Err(Error::new(ErrorKind::InvalidInput, ""));
		}
		self.writer.write_all(LOCAL_HEADER)?;
		self.writer.write_all(VERSION)?;
		self.writer.write_all(&[0b00001000, 0b00001000])?;
		self.writer.write_all(&Compression::None.to_le_bytes())?;
		self.writer.write_all(&date_time.to_le_bytes())?;
		self.writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
		self.writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
		self.writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
		self.writer.write_all(&(name.len() as u16).to_le_bytes())?;
		self.writer.write_all(&[0x00, 0x00])?;
		self.writer.write_all(name.as_bytes())?;
		let position = self.size;
		self.size += 30 + name.len() as u64;
		self.entries.push(Entry {
			crc: 0,
			date_time,
			name,
			position,
			size: self.size,
		});

		Ok(())
	}

	pub fn finish(&mut self) -> io::Result<()> {
		self.write_descriptor()?;
		let position = self.size;
		for entry in &self.entries {
			self.writer.write_all(CENTRAL_DIRECTORY_HEADER)?;
			self.writer.write_all(PLATFORM)?;
			self.writer.write_all(VERSION)?;
			self.writer.write_all(&[0b00001000, 0b00001000])?;
			self.writer.write_all(&Compression::None.to_le_bytes())?;
			self.writer.write_all(&entry.date_time.to_le_bytes())?;
			self.writer.write_all(&entry.crc.to_le_bytes())?;
			self.writer.write_all(&(entry.size as u32).to_le_bytes())?;
			self.writer.write_all(&(entry.size as u32).to_le_bytes())?;
			self.writer.write_all(&(entry.name.len() as u16).to_le_bytes())?;
			self.writer.write_all(&[0x00, 0x00])?;
			self.writer.write_all(&[0x00, 0x00])?;
			self.writer.write_all(&[0x00, 0x00])?;
			self.writer.write_all(&[0x00, 0x00])?;
			self.writer.write_all(&[0x00, 0x00, 0x00, 0x00])?;
			self.writer.write_all(&(entry.position as u32).to_le_bytes())?;
			self.writer.write_all(entry.name.as_bytes())?;
			self.size += 46 + entry.name.len() as u64;
		}

		let number_entries = self.entries.len() as u16;
		let size = (self.size - position) as u32;
		self.writer.write_all(END_CENTRAL_DIRECTORY)?;
		self.writer.write_all(&[0x00, 0x00])?;
		self.writer.write_all(&[0x00, 0x00])?;
		self.writer.write_all(&number_entries.to_le_bytes())?;
		self.writer.write_all(&number_entries.to_le_bytes())?;
		self.writer.write_all(&size.to_le_bytes())?;
		self.writer.write_all(&(position as u32).to_le_bytes())?;
		self.writer.write_all(&[0x00, 0x00])?;

		Ok(())
	}

	fn write_descriptor(&mut self) -> io::Result<()> {
		let Some(entry) = self.entries.last_mut() else {
			return Ok(());
		};
		#[cfg(feature = "crc")]
		{
			entry.crc = self.crc.clone().finalize();
			self.crc.reset();
		}
		entry.size = self.size - entry.size;
		self.writer.write_all(&entry.crc.to_le_bytes())?;
		self.writer.write_all(&(entry.size as u32).to_le_bytes())?;
		self.writer.write_all(&(entry.size as u32).to_le_bytes())?;
		self.size += 12;

		Ok(())
	}
}

impl<W: Write> Write for Writer<W> {
	fn write(&mut self, data: &[u8]) -> io::Result<usize> {
		let size = self.writer.write(data)?;
		#[cfg(feature = "crc")]
		self.crc.update(data);
		self.size += size as u64;
		Ok(size)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.writer.flush()
	}
}
