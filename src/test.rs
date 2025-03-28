use crate::{Compression, DateTime, Zip};
use std::io::Write;
#[cfg(feature = "tokio")]
use tokio::io::AsyncWriteExt;

const NO_ENTRIES: &[u8] = &[
	0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[cfg(feature = "deflate")]
const ONE_COMPRESSED_ENTRY: &[u8] = &[
	0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0b00001000, 0b00001000, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
	b'1', b'.', b't', b'x', b't', 0x0A, 0xCE, 0xCF, 0x4D, 0x55, 0x48, 0x49, 0x2C, 0x49, 0xE4, 0x02,
	0x00, 0x00, 0x00, 0xFF, 0xFF, 0x03, 0x00, 0xC9, 0xFA, 0x5C, 0x87, 0x12, 0x00, 0x00, 0x00, 0x0A,
	0x00, 0x00, 0x00, 0x50, 0x4B, 0x01, 0x02, 0x00, 0x00, 0x14, 0x00, 0b00001000, 0b00001000, 0x08,
	0x00, 0x00, 0x00, 0x00, 0x00, 0xC9, 0xFA, 0x5C, 0x87, 0x12, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00,
	0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, b'1', b'.', b't', b'x', b't', 0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00,
	0x01, 0x00, 0x01, 0x00, 0x33, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[cfg(feature = "deflate")]
#[cfg(not(feature = "crc"))]
macro_rules! one_compressed_entry_no_crc {
	() => {{
		let mut data = ONE_COMPRESSED_ENTRY.to_vec();
		data[53..57].copy_from_slice(&[0, 0, 0, 0]);
		data[81..85].copy_from_slice(&[0, 0, 0, 0]);
		data
	}};
}

const ONE_UNCOMPRESSED_ENTRY: &[u8] = &[
	0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0b00001000, 0b00001000, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
	b'1', b'.', b't', b'x', b't', b'S', b'o', b'm', b'e', b' ', b'd', b'a', b't', b'a', b'\n',
	0xC9, 0xFA, 0x5C, 0x87, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x50, 0x4B, 0x01, 0x02,
	0x00, 0x00, 0x14, 0x00, 0b00001000, 0b00001000, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC9, 0xFA,
	0x5C, 0x87, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'1', b'.', b't', b'x',
	b't', 0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x33, 0x00, 0x00,
	0x00, 0x39, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[cfg(not(feature = "crc"))]
macro_rules! one_uncompressed_entry_no_crc {
	() => {{
		let mut data = ONE_UNCOMPRESSED_ENTRY.to_vec();
		data[45..49].copy_from_slice(&[0, 0, 0, 0]);
		data[73..77].copy_from_slice(&[0, 0, 0, 0]);
		data
	}};
}

const TWO_ENTRIES: &[u8] = &[
	0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0b00001000, 0b00001000, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
	b'1', b'.', b't', b'x', b't', b'S', b'o', b'm', b'e', b' ', b'd', b'a', b't', b'a', b'\n',
	0xC9, 0xFA, 0x5C, 0x87, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x50, 0x4B, 0x03, 0x04,
	0x14, 0x00, 0b00001000, 0b00001000, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, b'2', b'.', b't', b'x',
	b't', b'S', b'o', b'm', b'e', b' ', b'm', b'o', b'r', b'e', b' ', b'd', b'a', b't', b'a',
	b'\n', 0x2F, 0x9B, 0xBB, 0x5A, 0x0F, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x00, 0x00, 0x50, 0x4B,
	0x01, 0x02, 0x00, 0x00, 0x14, 0x00, 0b00001000, 0b00001000, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
	0xC9, 0xFA, 0x5C, 0x87, 0x0A, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'1', b'.',
	b't', b'x', b't', 0x50, 0x4B, 0x01, 0x02, 0x00, 0x00, 0x14, 0x00, 0b00001000, 0b00001000, 0x00,
	0x00, 0x00, 0x00, 0x00, 0x00, 0x2F, 0x9B, 0xBB, 0x5A, 0x0F, 0x00, 0x00, 0x00, 0x0F, 0x00, 0x00,
	0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x39,
	0x00, 0x00, 0x00, b'2', b'.', b't', b'x', b't', 0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00,
	0x02, 0x00, 0x02, 0x00, 0x66, 0x00, 0x00, 0x00, 0x77, 0x00, 0x00, 0x00, 0x00, 0x00,
];

#[cfg(not(feature = "crc"))]
macro_rules! two_entries_no_crc {
	() => {{
		let mut data = TWO_ENTRIES.to_vec();
		data[45..49].copy_from_slice(&[0, 0, 0, 0]);
		data[107..111].copy_from_slice(&[0, 0, 0, 0]);
		data[135..139].copy_from_slice(&[0, 0, 0, 0]);
		data[186..190].copy_from_slice(&[0, 0, 0, 0]);
		data
	}};
}

#[test]
fn no_entries() {
	let mut data = Vec::new();
	let writer = Zip::new(&mut data);
	assert!(writer.finish().is_ok());
	assert_eq!(data, NO_ENTRIES);
}

#[test]
#[cfg(feature = "deflate")]
fn one_compressed_entry() {
	let mut data = Vec::new();
	let mut writer = Zip::new(&mut data);
	assert!(writer.create_entry("1.txt", Compression::Deflate, DateTime::default()).is_ok());
	assert!(writer.write_all(b"Some data\n").is_ok());
	assert!(writer.finish().is_ok());
	#[cfg(feature = "crc")]
	assert_eq!(data, ONE_COMPRESSED_ENTRY);
	#[cfg(not(feature = "crc"))]
	assert_eq!(data, one_compressed_entry_no_crc!());
}

#[test]
fn one_uncompressed_entry() {
	let mut data = Vec::new();
	let mut writer = Zip::new(&mut data);
	assert!(writer.create_entry("1.txt", Compression::None, DateTime::default()).is_ok());
	assert!(writer.write_all(b"Some data\n").is_ok());
	assert!(writer.finish().is_ok());
	#[cfg(feature = "crc")]
	assert_eq!(data, ONE_UNCOMPRESSED_ENTRY);
	#[cfg(not(feature = "crc"))]
	assert_eq!(data, one_uncompressed_entry_no_crc!());
}

#[test]
fn two_uncompressed_entries() {
	let mut data = Vec::new();
	let mut writer = Zip::new(&mut data);
	assert!(writer.create_entry("1.txt", Compression::None, DateTime::default()).is_ok());
	assert!(writer.write_all(b"Some data\n").is_ok());
	assert!(writer.create_entry("2.txt", Compression::None, DateTime::default()).is_ok());
	assert!(writer.write_all(b"Some more data\n").is_ok());
	assert!(writer.finish().is_ok());
	#[cfg(feature = "crc")]
	assert_eq!(data, TWO_ENTRIES);
	#[cfg(not(feature = "crc"))]
	assert_eq!(data, two_entries_no_crc!());
}

#[tokio::test]
#[cfg(feature = "tokio")]
async fn tokio_no_entries() {
	let mut data = Vec::new();
	let mut writer = crate::tokio::Zip::new(&mut data);
	assert!(writer.finish().await.is_ok());
	assert_eq!(data, NO_ENTRIES);
}

#[tokio::test]
#[cfg(feature = "tokio")]
async fn tokio_one_uncompressed_entry() {
	let mut data = Vec::new();
	let mut writer = crate::tokio::Zip::new(&mut data);
	assert!(writer.create_entry("1.txt", Compression::None, DateTime::default()).await.is_ok());
	assert!(writer.write_all(b"Some data\n").await.is_ok());
	assert!(writer.finish().await.is_ok());
	#[cfg(feature = "crc")]
	assert_eq!(data, ONE_UNCOMPRESSED_ENTRY);
	#[cfg(not(feature = "crc"))]
	assert_eq!(data, one_uncompressed_entry_no_crc!());
}

#[tokio::test]
#[cfg(feature = "deflate")]
#[cfg(feature = "tokio")]
async fn tokio_one_compressed_entry() {
	let mut data = Vec::new();
	let mut writer = crate::tokio::Zip::new(&mut data);
	assert!(writer.create_entry("1.txt", Compression::Deflate, DateTime::default()).await.is_ok());
	assert!(writer.write_all(b"Some data\n").await.is_ok());
	assert!(writer.finish().await.is_ok());
	#[cfg(feature = "crc")]
	assert_eq!(data, ONE_COMPRESSED_ENTRY);
	#[cfg(not(feature = "crc"))]
	assert_eq!(data, one_compressed_entry_no_crc!());
}

#[tokio::test]
#[cfg(feature = "tokio")]
async fn tokio_two_uncompressed_entries() {
	let mut data = Vec::new();
	let mut writer = crate::tokio::Zip::new(&mut data);
	assert!(writer.create_entry("1.txt", Compression::None, DateTime::default()).await.is_ok());
	assert!(writer.write_all(b"Some data\n").await.is_ok());
	assert!(writer.create_entry("2.txt", Compression::None, DateTime::default()).await.is_ok());
	assert!(writer.write_all(b"Some more data\n").await.is_ok());
	assert!(writer.finish().await.is_ok());
	#[cfg(feature = "crc")]
	assert_eq!(data, TWO_ENTRIES);
	#[cfg(not(feature = "crc"))]
	assert_eq!(data, two_entries_no_crc!());
}
