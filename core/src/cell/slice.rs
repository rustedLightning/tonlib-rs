use std::io::Cursor;
use std::sync::Arc;

use bitstream_io::{BigEndian, BitRead, BitReader};

use crate::cell::util::BitReadExt;
use crate::cell::{ArcCell, Cell, MapTonCellError, TonCellError};

#[derive(Debug, Clone, PartialEq)]
pub struct CellSlice {
    pub cell: ArcCell,
    pub start_bit: usize,
    pub end_bit: usize,
    pub start_ref: usize,
    pub end_ref: usize,
}

impl CellSlice {
    pub fn new(
        cell: &ArcCell,
        start_bit: usize,
        end_bit: usize,
        start_ref: usize,
        end_ref: usize,
    ) -> Result<CellSlice, TonCellError> {
        if end_bit < start_bit || end_bit > cell.bit_len {
            return Err(TonCellError::CellParserError(format!(
                "Invalid bit offsets: start: {}, end: {}, bit_len: {}",
                start_bit, end_bit, cell.bit_len
            )));
        }
        if end_ref < start_ref || end_ref > cell.references.len() {
            return Err(TonCellError::CellParserError(format!(
                "Invalid references: start: {}, end: {}, count: {}",
                start_bit,
                end_bit,
                cell.references.len()
            )));
        }
        Ok(CellSlice {
            cell: cell.clone(),
            start_bit,
            end_bit,
            start_ref,
            end_ref,
        })
    }

    pub fn new_with_offset(cell: &Cell, offset: usize) -> Result<CellSlice, TonCellError> {
        CellSlice::new(
            &Arc::new(cell.clone()),
            offset,
            cell.bit_len,
            0,
            cell.references.len(),
        )
    }

    pub fn full_cell(cell: Cell) -> Result<CellSlice, TonCellError> {
        let bit_len = cell.bit_len;
        let ref_count = cell.references.len();
        Ok(CellSlice {
            cell: Arc::new(cell),
            start_bit: 0,
            end_bit: bit_len,
            start_ref: 0,
            end_ref: ref_count,
        })
    }

    pub fn reference(&self, idx: usize) -> Result<&ArcCell, TonCellError> {
        if idx > self.end_ref - self.start_ref {
            return Err(TonCellError::InvalidIndex {
                idx,
                ref_count: self.end_ref - self.start_ref,
            });
        }
        self.cell
            .references
            .get(self.start_ref + idx)
            .ok_or(TonCellError::InvalidIndex {
                idx,
                ref_count: self.end_ref - self.start_ref,
            })
    }

    /// Converts the slice to full `Cell` dropping references to original cell.
    pub fn into_cell(&self) -> Result<Cell, TonCellError> {
        let bit_len = self.end_bit - self.start_bit;
        let total_bytes = bit_len.div_ceil(8);
        let mut data = vec![0u8; total_bytes];
        let cursor = Cursor::new(&self.cell.data);
        let mut bit_reader: BitReader<Cursor<&Vec<u8>>, BigEndian> =
            BitReader::endian(cursor, BigEndian);
        bit_reader
            .skip(self.start_bit as u32)
            .map_cell_parser_error()?;
        bit_reader.read_bits(bit_len, data.as_mut_slice())?;

        Cell::new(
            data,
            bit_len,
            self.cell.references[self.start_ref..self.end_ref].to_vec(),
            false,
        )
    }
}
