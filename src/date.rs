#[derive(Default)]
pub struct DateTime;

impl DateTime {
	pub fn to_le_bytes(&self) -> [u8; 4] {
		[0, 0, 0, 0]
	}
}
