use std::io::{self, Write};

pub(crate) struct BoundedBytes {
    bytes: Vec<u8>,
    max_len: usize,
    rejected_len: Option<usize>,
}

impl BoundedBytes {
    pub(crate) fn new(max_len: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_len,
            rejected_len: None,
        }
    }

    pub(crate) const fn limit_exceeded(&self) -> bool {
        self.rejected_len.is_some()
    }

    pub(crate) const fn rejected_len(&self) -> Option<usize> {
        self.rejected_len
    }

    pub(crate) fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedBytes {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let Some(required) = self.bytes.len().checked_add(input.len()) else {
            self.rejected_len = Some(usize::MAX);
            return Err(io::Error::other("encoded image byte budget exceeded"));
        };
        if required > self.max_len {
            self.rejected_len = Some(required);
            return Err(io::Error::other("encoded image byte budget exceeded"));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_the_write_that_would_cross_the_limit() {
        let mut bytes = BoundedBytes::new(4);
        bytes.write_all(&[1, 2, 3]).unwrap();
        assert!(bytes.write_all(&[4, 5]).is_err());
        assert!(bytes.limit_exceeded());
        assert_eq!(bytes.rejected_len(), Some(5));
        assert_eq!(bytes.into_inner(), vec![1, 2, 3]);
    }
}
