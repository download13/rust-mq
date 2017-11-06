use std::io::{self, Cursor, Read, Write};
use std::sync::{Mutex, Arc};
use std::net::{SocketAddr, Shutdown};
use std::time::Duration;

pub type MockCursor = Cursor<Vec<u8>>;

#[derive(Clone)]
pub struct MockStream {
    reader: Arc<Mutex<MockCursor>>,
    writer: Arc<Mutex<MockCursor>>
}

impl MockStream {
    pub fn new() -> MockStream {
        MockStream {
            reader: Arc::new(Mutex::new(MockCursor::new(Vec::new()))),
            writer: Arc::new(Mutex::new(MockCursor::new(Vec::new())))
        }
    }

    pub fn with_vec(vec: Vec<u8>) -> MockStream {
        MockStream {
            reader: Arc::new(Mutex::new(MockCursor::new(vec))),
            writer: Arc::new(Mutex::new(MockCursor::new(Vec::new())))
        }
    }

    pub fn take_vec(&mut self) -> Vec<u8> {
        let mut cursor = self.writer.lock().unwrap();
        let vec = cursor.get_ref().to_vec();
        cursor.set_position(0);
        cursor.get_mut().clear();
        vec
    }

    pub fn next_vec(&mut self, vec: Vec<u8>) {
        let mut cursor = self.reader.lock().unwrap();
        cursor.set_position(0);
        cursor.get_mut().clear();
        cursor.get_mut().extend_from_slice(vec.as_slice());
    }

    pub fn swap(&mut self) {
        let mut cur_write = self.writer.lock().unwrap();
        let mut cur_read = self.reader.lock().unwrap();
        let vec_write = cur_write.get_ref().to_vec();
        let vec_read = cur_read.get_ref().to_vec();
        cur_write.set_position(0);
        cur_read.set_position(0);
        cur_write.get_mut().clear();
        cur_read.get_mut().clear();
        // swap cursors
        cur_read.get_mut().extend_from_slice(vec_write.as_slice());
        cur_write.get_mut().extend_from_slice(vec_read.as_slice());
    }

    pub fn try_clone(&self) -> io::Result<MockStream> {
        panic!("mock try_clone called");
    }

    pub fn get_ref(&self) -> &MockStream {
        panic!("mock get_ref called");
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        panic!("mock peer_addr called");
    }

    pub fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
        panic!("mock shutdown called");
    }

    pub fn set_read_timeout(&self, _dur: Option<Duration>) -> io::Result<()> {
        panic!("mock set_read_timeout called");
    }

    pub fn set_write_timeout(&self, _dur: Option<Duration>) -> io::Result<()> {
        panic!("mock set_write_timeout called");
    }
}

impl Write for MockStream {
    fn write(&mut self, msg: &[u8]) -> io::Result<usize> {
        self.writer.lock().unwrap().write(msg)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.lock().unwrap().flush()
    }
}

impl Read for MockStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.lock().unwrap().read(buf)
    }
}

#[cfg(test)]
mod test {
    use std::io::{Read, Write};
    use super::MockStream;

    #[test]
    fn write_take_test() {
        let mut mock = MockStream::new();
        // write to mock stream
        mock.write(&[1,2,3]).unwrap();
        assert_eq!(mock.take_vec(), vec![1,2,3]);
    }

    #[test]
    fn read_with_vec_test() {
        let mut mock = MockStream::with_vec(vec![4,5]);
        let mut vec = Vec::new();
        mock.read_to_end(&mut vec).unwrap();
        assert_eq!(vec, vec![4,5]);
    }

    #[test]
    fn clone_test() {
        let mut mock = MockStream::new();
        let mut clonned = mock.clone();
        mock.write(&[6,7]).unwrap();
        assert_eq!(clonned.take_vec(), vec![6,7]);
    }

    #[test]
    fn swap_test() {
        let mut mock = MockStream::new();
        let mut vec = Vec::new();
        mock.write(&[8,9,10]).unwrap();
        mock.swap();
        mock.read_to_end(&mut vec).unwrap();
        assert_eq!(vec, vec![8,9,10]);
    }
}
