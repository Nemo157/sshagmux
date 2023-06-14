// use ssh_encoding::{Decode, Reader};

// trait SshEncodingResultExt {
//     type Ok;
//     fn maybe(self) -> ssh_encoding::Result<Option<Self::Ok>>;
// }
//
// impl<T> SshEncodingResultExt for ssh_encoding::Result<T> {
//     type Ok = T;
//     fn maybe(self) -> ssh_encoding::Result<Option<Self::Ok>> {
//         match self {
//             Ok(value) => Ok(Some(value)),
//             Err(ssh_encoding::Error::Length) => Ok(None),
//             Err(e) => Err(e),
//         }
//     }
// }

// struct Local<T>(T);
//
// impl Reader for Local<&mut Bytes> {
//     fn read<'o>(&mut self, out: &'o mut [u8]) -> Result<&'o [u8]> {
//         (self.len() >= out.len()).then(|| out.copy_from_slice(&self.split_to(out.len()))).ok_or_else(ssh_encoding::Error::Length)
//     }
//     fn remaining_len(&self) -> usize {
//         self.len()
//     }
// }
//
// impl Reader for Local<&mut BytesMut> {
//     fn read<'o>(&mut self, out: &'o mut [u8]) -> Result<&'o [u8]> {
//         (self.len() >= out.len()).then(|| out.copy_from_slice(&self.split_to(out.len()))).ok_or_else(ssh_encoding::Error::Length)
//     }
//     fn remaining_len(&self) -> usize {
//         self.len()
//     }
// }

