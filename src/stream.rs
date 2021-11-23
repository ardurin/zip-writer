use crate::{
	Compression, DateTime, Entry, CENTRAL_DIRECTORY_HEADER, END_CENTRAL_DIRECTORY, LOCAL_HEADER,
	PLATFORM, VERSION,
};
#[cfg(feature = "crc")]
use crc32fast::Hasher;
use std::{
	io::{self, Error, ErrorKind},
	pin::Pin,
	task::{Context, Poll},
};
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub struct Writer<W: AsyncWrite + Unpin> {
	#[cfg(feature = "crc")]
	crc: Hasher,
	entries: Vec<Entry>,
	size: u64,
	writer: W,
}

impl<W: AsyncWrite + Unpin> Writer<W> {
	pub fn new(writer: W) -> Self {
		Self {
			#[cfg(feature = "crc")]
			crc: Hasher::new(),
			entries: Vec::new(),
			size: 0,
			writer,
		}
	}

	pub async fn create_entry<T: Into<String>>(
		&mut self,
		name: T,
		_: Compression,
		date_time: DateTime,
	) -> io::Result<()> {
		self.write_descriptor().await?;
		let name = name.into();
		if name.len() > u16::MAX.into() {
			return Err(Error::new(ErrorKind::InvalidInput, ""));
		}
		self.writer.write_all(LOCAL_HEADER).await?;
		self.writer.write_all(VERSION).await?;
		self.writer.write_all(&[0b00001000, 0b00001000]).await?;
		self.writer.write_all(&Compression::None.to_le_bytes()).await?;
		self.writer.write_all(&date_time.to_le_bytes()).await?;
		self.writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
		self.writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
		self.writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
		self.writer.write_all(&(name.len() as u16).to_le_bytes()).await?;
		self.writer.write_all(&[0x00, 0x00]).await?;
		self.writer.write_all(name.as_bytes()).await?;
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

	pub async fn finish(&mut self) -> io::Result<()> {
		self.write_descriptor().await?;
		let position = self.size;
		for entry in &self.entries {
			self.writer.write_all(CENTRAL_DIRECTORY_HEADER).await?;
			self.writer.write_all(PLATFORM).await?;
			self.writer.write_all(VERSION).await?;
			self.writer.write_all(&[0b00001000, 0b00001000]).await?;
			self.writer.write_all(&Compression::None.to_le_bytes()).await?;
			self.writer.write_all(&entry.date_time.to_le_bytes()).await?;
			self.writer.write_all(&entry.crc.to_le_bytes()).await?;
			self.writer.write_all(&(entry.size as u32).to_le_bytes()).await?;
			self.writer.write_all(&(entry.size as u32).to_le_bytes()).await?;
			self.writer.write_all(&(entry.name.len() as u16).to_le_bytes()).await?;
			self.writer.write_all(&[0x00, 0x00]).await?;
			self.writer.write_all(&[0x00, 0x00]).await?;
			self.writer.write_all(&[0x00, 0x00]).await?;
			self.writer.write_all(&[0x00, 0x00]).await?;
			self.writer.write_all(&[0x00, 0x00, 0x00, 0x00]).await?;
			self.writer.write_all(&(entry.position as u32).to_le_bytes()).await?;
			self.writer.write_all(entry.name.as_bytes()).await?;
			self.size += 46 + entry.name.len() as u64;
		}
		let number_entries = self.entries.len() as u16;
		let size = (self.size - position) as u32;
		self.writer.write_all(END_CENTRAL_DIRECTORY).await?;
		self.writer.write_all(&[0x00, 0x00]).await?;
		self.writer.write_all(&[0x00, 0x00]).await?;
		self.writer.write_all(&number_entries.to_le_bytes()).await?;
		self.writer.write_all(&number_entries.to_le_bytes()).await?;
		self.writer.write_all(&size.to_le_bytes()).await?;
		self.writer.write_all(&(position as u32).to_le_bytes()).await?;
		self.writer.write_all(&[0x00, 0x00]).await?;

		Ok(())
	}

	async fn write_descriptor(&mut self) -> io::Result<()> {
		let Some(entry) = self.entries.last_mut() else {
			return Ok(());
		};
		#[cfg(feature = "crc")]
		{
			entry.crc = self.crc.clone().finalize();
			self.crc.reset();
		}
		entry.size = self.size - entry.size;
		self.writer.write_all(&entry.crc.to_le_bytes()).await?;
		self.writer.write_all(&(entry.size as u32).to_le_bytes()).await?;
		self.writer.write_all(&(entry.size as u32).to_le_bytes()).await?;
		self.size += 12;

		Ok(())
	}
}

impl<W> AsyncWrite for Writer<W>
where
	W: AsyncWrite + Unpin,
{
	fn poll_write(
		mut self: Pin<&mut Self>,
		context: &mut Context<'_>,
		data: &[u8],
	) -> Poll<io::Result<usize>> {
		let status = Pin::new(&mut self.writer).poll_write(context, data);
		if let Poll::Ready(Ok(size)) = status {
			#[cfg(feature = "crc")]
			self.crc.update(data);
			self.size += size as u64;
		}
		status
	}

	fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
		Pin::new(&mut self.writer).poll_flush(context)
	}

	fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
		Pin::new(&mut self.writer).poll_shutdown(context)
	}
}
