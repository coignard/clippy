use crate::acs::AcsError;

pub(super) struct Reader<'a> {
    file: &'a [u8],
    at: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(file: &'a [u8]) -> Self {
        Self {
            file,
            at: 0,
            end: file.len(),
        }
    }

    pub(super) fn located(&mut self) -> Result<Self, AcsError> {
        let offset = self.length()?;
        let size = self.length()?;
        let end = offset.checked_add(size).ok_or(AcsError::Truncated)?;
        if end > self.file.len() {
            return Err(AcsError::Truncated);
        }
        Ok(Self {
            file: self.file,
            at: offset,
            end,
        })
    }

    pub(super) fn bytes(&mut self, count: usize) -> Result<&'a [u8], AcsError> {
        let end = self.at.checked_add(count).ok_or(AcsError::Truncated)?;
        if end > self.end {
            return Err(AcsError::Truncated);
        }
        let bytes = &self.file[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    pub(super) fn skip(&mut self, count: usize) -> Result<(), AcsError> {
        self.bytes(count).map(drop)
    }

    pub(super) fn u8(&mut self) -> Result<u8, AcsError> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, AcsError> {
        self.array().map(u16::from_le_bytes)
    }

    pub(super) fn i16(&mut self) -> Result<i16, AcsError> {
        self.array().map(i16::from_le_bytes)
    }

    pub(super) fn u32(&mut self) -> Result<u32, AcsError> {
        self.array().map(u32::from_le_bytes)
    }

    pub(super) fn length(&mut self) -> Result<usize, AcsError> {
        usize::try_from(self.u32()?).map_err(|_| AcsError::Truncated)
    }

    pub(super) fn flag(&mut self) -> Result<bool, AcsError> {
        Ok(self.u8()? != 0)
    }

    pub(super) fn string(&mut self) -> Result<String, AcsError> {
        let count = self.length()?;
        if count == 0 {
            return Ok(String::new());
        }
        let units: Vec<u16> = self
            .bytes(count.checked_mul(2).ok_or(AcsError::Truncated)?)?
            .as_chunks()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        self.skip(2)?;
        Ok(String::from_utf16_lossy(&units))
    }

    pub(super) fn list<T>(
        &mut self,
        count: usize,
        mut item: impl FnMut(&mut Self) -> Result<T, AcsError>,
    ) -> Result<Vec<T>, AcsError> {
        (0..count).map(|_| item(self)).collect()
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], AcsError> {
        let bytes = self.bytes(N)?;
        Ok(bytes
            .try_into()
            .expect("bytes should return exactly N bytes"))
    }

    pub(super) fn rest(&mut self) -> &'a [u8] {
        let rest = &self.file[self.at..self.end];
        self.at = self.end;
        rest
    }
}
