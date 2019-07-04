use std::ops::Range;

use crate::{Error, Result};

pub trait FromData: Sized {
    /// Parses an object from a raw data.
    ///
    /// This method **must** not panic and **must** not read past the bounds.
    fn parse(s: &mut SafeStream) -> Self;

    /// Returns an object size in raw data.
    ///
    /// `mem::size_of` by default.
    ///
    /// Reimplement when size of `Self` != size of a raw data.
    /// For example, when you parsing u16, but storing it as u8.
    /// In this case `size_of::<Self>()` == 1, but `FromData::raw_size()` == 2.
    fn raw_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

impl FromData for u8 {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        s.data[0]
    }
}

impl FromData for i8 {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        s.data[0] as i8
    }
}

impl FromData for u16 {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        let d = s.data;
        (d[0] as u16) << 8 | d[1] as u16
    }
}

impl FromData for i16 {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        let d = s.data;
        ((d[0] as u16) << 8 | d[1] as u16) as i16
    }
}

impl FromData for u32 {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        let d = s.data;
        (d[0] as u32) << 24 | (d[1] as u32) << 16 | (d[2] as u32) << 8 | d[3] as u32
    }
}


pub trait TryFromData: Sized {
    /// Parses an object from a raw data.
    fn try_parse(s: &mut SafeStream) -> Result<Self>;

    /// Returns an object size in raw data.
    ///
    /// `mem::size_of` by default.
    ///
    /// Reimplement when size of `Self` != size of a raw data.
    /// For example, when you parsing u16, but storing it as u8.
    /// In this case `size_of::<Self>()` == 1, but `TryFromData::raw_size()` == 2.
    fn raw_size() -> usize {
        std::mem::size_of::<Self>()
    }
}


// Like `usize`, but for font.
pub trait FSize {
    fn to_usize(&self) -> usize;
}

impl FSize for u16 {
    #[inline]
    fn to_usize(&self) -> usize { *self as usize }
}

impl FSize for u32 {
    #[inline]
    fn to_usize(&self) -> usize { *self as usize }
}


#[derive(Clone, Copy)]
pub struct LazyArray<'a, T> {
    data: &'a [u8],
    phantom: std::marker::PhantomData<T>,
}

impl<'a, T: FromData> LazyArray<'a, T> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        LazyArray {
            data,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn at<L: FSize>(&self, index: L) -> T {
        let start = index.to_usize() * T::raw_size();
        let end = start + T::raw_size();
        let mut s = SafeStream::new(&self.data[start..end]);
        T::parse(&mut s)
    }

    pub fn get<L: FSize>(&self, index: L) -> Option<T> {
        if index.to_usize() < self.len() {
            let start = index.to_usize() * T::raw_size();
            let end = start + T::raw_size();
            let mut s = SafeStream::new(&self.data[start..end]);
            Some(T::parse(&mut s))
        } else {
            None
        }
    }

    #[inline]
    pub fn last(&self) -> Option<T> {
        if !self.is_empty() {
            self.get(self.len() as u32 - 1)
        } else {
            None
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len() / T::raw_size()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn binary_search_by<F>(&self, mut f: F) -> Option<T>
        where F: FnMut(&T) -> std::cmp::Ordering
    {
        // Based on Rust std implementation.

        use std::cmp::Ordering;

        let mut size = self.len() as u32;
        if size == 0 {
            return None;
        }

        let mut base = 0;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            // mid is always in [0, size), that means mid is >= 0 and < size.
            // mid >= 0: by definition
            // mid < size: mid = size / 2 + size / 4 + size / 8 ...
            let cmp = f(&self.at(mid));
            base = if cmp == Ordering::Greater { base } else { mid };
            size -= half;
        }

        // base is always in [0, size) because base <= mid.
        let value = self.at(base);
        let cmp = f(&value);
        if cmp == Ordering::Equal { Some(value) } else { None }
    }
}

impl<'a, T: FromData + std::fmt::Debug + Copy> std::fmt::Debug for LazyArray<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_list().entries(self.into_iter()).finish()
    }
}

impl<'a, T: FromData> IntoIterator for LazyArray<'a, T> {
    type Item = T;
    type IntoIter = LazyArrayIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        LazyArrayIter {
            data: self,
            offset: 0,
        }
    }
}


pub struct LazyArrayIter<'a, T> {
    data: LazyArray<'a, T>,
    offset: u32,
}

impl<'a, T: FromData> Iterator for LazyArrayIter<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.offset as usize == self.data.len() {
            return None;
        }

        let index = self.offset;
        self.offset += 1;
        self.data.get(index)
    }
}


#[derive(Clone, Copy)]
pub struct Stream<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Stream<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        Stream {
            data,
            offset: 0,
        }
    }

    #[inline]
    fn get_data(&self, range: Range<usize>) -> Result<&'a [u8]> {
        self.data.get(range.clone())
            .ok_or_else(|| Error::ReadOutOfBounds(range.end, self.data.len()))
    }

    #[inline]
    pub fn at_end(&self) -> bool {
        self.offset == self.data.len()
    }

    #[inline]
    pub fn jump_to_end(&mut self) {
        self.offset = self.data.len();
    }

    #[inline]
    pub fn offset(&self) -> usize {
        self.offset
    }

    #[inline]
    pub fn tail(&self) -> Result<&'a [u8]> {
        self.get_data(self.offset..self.data.len())
    }

    #[inline]
    pub fn skip<T: FromData>(&mut self) {
        self.offset += T::raw_size();
    }

    #[inline]
    pub fn skip_len<L: FSize>(&mut self, len: L) {
        self.offset += len.to_usize();
    }

    #[inline]
    pub fn read<T: FromData>(&mut self) -> Result<T> {
        let start = self.offset;
        self.offset += T::raw_size();
        let end = self.offset;

        let data = self.get_data(start..end)?;
        let mut s = SafeStream::new(data);
        Ok(T::parse(&mut s))
    }

    #[inline]
    pub fn try_read<T: TryFromData>(&mut self) -> Result<T> {
        let start = self.offset;
        self.offset += T::raw_size();
        let end = self.offset;

        let data = self.get_data(start..end)?;
        let mut s = SafeStream::new(data);
        T::try_parse(&mut s)
    }

    #[inline]
    pub fn read_at<T: FromData>(data: &[u8], mut offset: usize) -> Result<T> {
        let start = offset;
        offset += T::raw_size();
        let end = offset;

        let data = data.get(start..end)
            .ok_or_else(|| Error::ReadOutOfBounds(end, data.len()))?;

        let mut s = SafeStream::new(data);
        Ok(T::parse(&mut s))
    }

    #[inline]
    pub fn read_bytes<L: FSize>(&mut self, len: L) -> Result<&'a [u8]> {
        let offset = self.offset;
        self.offset += len.to_usize();
        self.get_data(offset..(offset + len.to_usize()))
    }

    #[inline]
    pub fn read_array<T: FromData, L: FSize>(&mut self, len: L) -> Result<LazyArray<'a, T>> {
        let len = len.to_usize() * T::raw_size();
        let data = self.read_bytes(len as u32)?;
        Ok(LazyArray::new(data))
    }

    pub fn read_f2_14(&mut self) -> Result<f32> {
        Ok(self.read::<i16>()? as f32 / 16384.0)
    }
}


/// A "safe" stream.
///
/// Unlike `Stream`, `SafeStream` doesn't perform bounds checking on each read.
/// It leverages the type system, so we can sort of guarantee that
/// we do not read past the bounds.
///
/// For example, if we are iterating a `LazyArray` we already checked it's size
/// and we can't read past the bounds, so we can remove useless checks.
///
/// It's still not 100% guarantee, but it makes code easier to read and a bit faster.
/// And we still backed by the Rust's bounds checking.
#[derive(Clone, Copy)]
pub struct SafeStream<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> SafeStream<'a> {
    #[inline]
    pub fn new(data: &'a [u8]) -> Self {
        SafeStream {
            data,
            offset: 0,
        }
    }

    #[inline]
    pub fn skip<T: FromData>(&mut self) {
        self.offset += T::raw_size();
    }

    #[inline]
    pub fn read<T: FromData>(&mut self) -> T {
        let start = self.offset;
        self.offset += T::raw_size();
        let end = self.offset;
        let mut s = SafeStream::new(&self.data[start..end]);
        T::parse(&mut s)
    }

    #[inline]
    pub fn read_u24(&mut self) -> u32 {
        let d = self.data;
        let n = 0 << 24 | (d[0] as u32) << 16 | (d[1] as u32) << 8 | d[2] as u32;
        self.offset += 3;
        n
    }
}


#[derive(Clone, Copy, Debug)]
pub struct Offset32(pub u32);

impl FromData for Offset32 {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        Offset32(s.read())
    }
}

impl FromData for Option<Offset32> {
    #[inline]
    fn parse(s: &mut SafeStream) -> Self {
        let offset: Offset32 = s.read();
        if offset.0 != 0 { Some(offset) } else { None }
    }

    fn raw_size() -> usize {
        Offset32::raw_size()
    }
}