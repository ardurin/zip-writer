use crate::{
	Compression, DateTime, Entry, CENTRAL_DIRECTORY_HEADER, END_CENTRAL_DIRECTORY, LOCAL_HEADER,
	PLATFORM, VERSION,
};
#[cfg(feature = "deflate")]
use async_compression::tokio::write::DeflateEncoder;
#[cfg(feature = "crc")]
use crc32fast::Hasher;
use std::{
	io::{self, Error, ErrorKind},
	mem::replace,
	pin::Pin,
	task::{Context, Poll},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};

enum Writer<W: AsyncWrite + Unpin> {
	#[cfg(feature = "deflate")]
	Deflate(DeflateEncoder<W>),
	Raw(W),
	None,
}

pub struct Zip<W: AsyncWrite + Unpin> {
	#[cfg(feature = "crc")]
	crc: Hasher,
	cursor: u64,
	entries: Vec<Entry>,
	writer: Writer<W>,
}

impl<W: AsyncWrite + Unpin> Zip<W> {
	pub fn new(writer: W) -> Self {
		Self {
			#[cfg(feature = "crc")]
			crc: Hasher::new(),
			entries: Vec::new(),
			cursor: 0,
			writer: Writer::Raw(writer),
		}
	}

	pub async fn create_entry<T: Into<String>>(
		&mut self,
		name: T,
		compression: Compression,
		date_time: DateTime,
	) -> io::Result<()> {
		let name = name.into();
		if name.len() > u16::MAX.into() {
			return Err(Error::new(ErrorKind::InvalidInput, ""));
		}
		let mut writer = self.commit_previous().await?;
		writer.write_all(LOCAL_HEADER).await?;
		writer.write_all(VERSION).await?;
		writer.write_all(&[0b00001000, 0b00001000]).await?;
		writer.write_all(&compression.to_le_bytes()).await?;
		writer.write_all(&date_time.to_le_bytes()).await?;
		writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
		writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
		writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
		writer.write_all(&(name.len() as u16).to_le_bytes()).await?;
		writer.write_all(&[0x00, 0x00]).await?;
		writer.write_all(name.as_bytes()).await?;
		_ = replace(
			&mut self.writer,
			match compression {
				#[cfg(feature = "deflate")]
				Compression::Deflate => Writer::Deflate(DeflateEncoder::new(writer)),
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

	pub async fn finish(&mut self) -> io::Result<()> {
		let mut writer = self.commit_previous().await?;
		let position = self.cursor;
		for entry in &self.entries {
			writer.write_all(CENTRAL_DIRECTORY_HEADER).await?;
			writer.write_all(PLATFORM).await?;
			writer.write_all(VERSION).await?;
			writer.write_all(&[0b00001000, 0b00001000]).await?;
			writer.write_all(&entry.compression.to_le_bytes()).await?;
			writer.write_all(&entry.date_time.to_le_bytes()).await?;
			writer.write_all(&entry.crc.to_le_bytes()).await?;
			writer.write_all(&(entry.size as u32).to_le_bytes()).await?;
			writer.write_all(&(entry.raw_size as u32).to_le_bytes()).await?;
			writer.write_all(&(entry.name.len() as u16).to_le_bytes()).await?;
			writer.write_all(&[0x00, 0x00]).await?;
			writer.write_all(&[0x00, 0x00]).await?;
			writer.write_all(&[0x00, 0x00]).await?;
			writer.write_all(&[0x00, 0x00]).await?;
			writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
			writer.write_all(&(entry.position as u32).to_le_bytes()).await?;
			writer.write_all(entry.name.as_bytes()).await?;
			self.cursor += 46 + entry.name.len() as u64;
		}
		let number_entries = self.entries.len() as u16;
		let size = (self.cursor - position) as u32;
		writer.write_all(END_CENTRAL_DIRECTORY).await?;
		writer.write_all(&[0x00, 0x00]).await?;
		writer.write_all(&[0x00, 0x00]).await?;
		writer.write_all(&number_entries.to_le_bytes()).await?;
		writer.write_all(&number_entries.to_le_bytes()).await?;
		writer.write_all(&size.to_le_bytes()).await?;
		writer.write_all(&(position as u32).to_le_bytes()).await?;
		writer.write_all(&[0x00, 0x00]).await?;

		Ok(())
	}

	async fn commit_previous(&mut self) -> io::Result<W> {
		let writer = replace(&mut self.writer, Writer::None);
		let Some(entry) = self.entries.last_mut() else {
			return Ok(match writer {
				#[cfg(feature = "deflate")]
				Writer::Deflate(encoder) => encoder.into_inner(),
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
				encoder.flush().await?;
				encoder.shutdown().await?;
				let size = encoder.total_out();
				(encoder.into_inner(), size)
			}
			Writer::Raw(writer) => (writer, entry.raw_size),
			Writer::None => unreachable!(),
		};
		entry.size = size;
		writer.write_all(&entry.crc.to_le_bytes()).await?;
		writer.write_all(&(entry.size as u32).to_le_bytes()).await?;
		writer.write_all(&(entry.raw_size as u32).to_le_bytes()).await?;
		self.cursor = start + entry.size + 12;

		Ok(writer)
	}
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Zip<W> {
	fn poll_write(
		mut self: Pin<&mut Self>,
		context: &mut Context<'_>,
		data: &[u8],
	) -> Poll<io::Result<usize>> {
		let status = match &mut self.writer {
			#[cfg(feature = "deflate")]
			Writer::Deflate(writer) => Pin::new(writer).poll_write(context, data),
			Writer::Raw(writer) => Pin::new(writer).poll_write(context, data),
			Writer::None => unreachable!(),
		};
		if let Poll::Ready(Ok(size)) = status {
			#[cfg(feature = "crc")]
			self.crc.update(data);
			self.cursor += size as u64;
		}
		status
	}

	fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
		match &mut self.writer {
			#[cfg(feature = "deflate")]
			Writer::Deflate(writer) => Pin::new(writer).poll_flush(context),
			Writer::Raw(writer) => Pin::new(writer).poll_flush(context),
			Writer::None => unreachable!(),
		}
	}

	fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
		match &mut self.writer {
			#[cfg(feature = "deflate")]
			Writer::Deflate(writer) => Pin::new(writer).poll_shutdown(context),
			Writer::Raw(writer) => Pin::new(writer).poll_shutdown(context),
			Writer::None => unreachable!(),
		}
	}
}
