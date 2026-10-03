//! Read-only structure proof for an exact left reclaimed-layout UF2/BIN pair.
//! This proof does not establish device identity, recovery, or permission to write.

use sha2::{Digest, Sha256};
use std::fmt;

const START: u32 = 0x1000;
const END: u32 = 0x65000;
const FAMILY: u32 = 0x621e937a;
const PAGE: usize = 4096;
const PAYLOAD: usize = 256;
const MAX_BLOCKS: usize = (END - START) as usize / PAYLOAD;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Blocks,
    Magic,
    Family,
    Alignment,
    Numbering,
    ProtectedMemory,
    Coverage,
    Binary,
    ExactMatch,
    StackPointer,
    ResetVector,
    RecoveryMarker,
    OldSoftDevice,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Blocks => "UF2 must contain complete 512-byte blocks within the image limit",
            Self::Magic => "invalid UF2 magic",
            Self::Family => "requires the left migration application family and ordinary flags",
            Self::Alignment => "requires aligned 256-byte payloads",
            Self::Numbering => "invalid block numbering",
            Self::ProtectedMemory => "image would touch protected or unverified memory",
            Self::Coverage => "requires unique contiguous payloads at 0x1000",
            Self::Binary => "exact aligned BIN must overwrite the entire old S140 magic word",
            Self::ExactMatch => "UF2 must match exact BIN with FF padding through its final page",
            Self::StackPointer => "invalid application stack pointer",
            Self::ResetVector => "reset vector must be Thumb code within exact BIN coverage",
            Self::RecoveryMarker => "requires recovery marker at 0x1200",
            Self::OldSoftDevice => "old S140 magic must be absent at 0x3004",
        })
    }
}

impl std::error::Error for ValidationError {}

/// Constructed only by [`validate`]; it attests to bytes, never to a device.
#[derive(Debug, Clone)]
pub struct ValidatedImage {
    sha256: String,
    binary_sha256: String,
    binary_size: usize,
    padded_size: usize,
    stack_pointer: u32,
    reset_vector: u32,
}

impl ValidatedImage {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn binary_sha256(&self) -> &str {
        &self.binary_sha256
    }
    pub fn binary_size(&self) -> usize {
        self.binary_size
    }
    pub fn start(&self) -> u32 {
        START
    }
    pub fn family_id(&self) -> u32 {
        FAMILY
    }
    pub fn binary_end_exclusive(&self) -> u32 {
        START + self.binary_size as u32
    }
    pub fn end_exclusive(&self) -> u32 {
        START + self.padded_size as u32
    }
    pub fn blocks(&self) -> usize {
        self.padded_size / PAYLOAD
    }
    pub fn stack_pointer(&self) -> u32 {
        self.stack_pointer
    }
    pub fn reset_vector(&self) -> u32 {
        self.reset_vector
    }
    pub fn touched_pages(&self) -> impl Iterator<Item = u32> + '_ {
        (START..self.end_exclusive()).step_by(PAGE)
    }
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked field bounds"),
    )
}

pub fn validate(image: &[u8], binary: &[u8]) -> Result<ValidatedImage, ValidationError> {
    use ValidationError::*;
    if image.is_empty() || !image.len().is_multiple_of(512) || image.len() / 512 > MAX_BLOCKS {
        return Err(Blocks);
    }
    let count = image.len() / 512;
    let mut numbered = vec![false; count];
    let mut blocks = Vec::with_capacity(count);
    for block in image.as_chunks::<512>().0 {
        if word(block, 0) != 0x0a324655
            || word(block, 4) != 0x9e5d5157
            || word(block, 508) != 0x0ab16f30
        {
            return Err(Magic);
        }
        if word(block, 8) != 0x2000 || word(block, 28) != FAMILY {
            return Err(Family);
        }
        let address = word(block, 12);
        if word(block, 16) != PAYLOAD as u32 || !address.is_multiple_of(PAYLOAD as u32) {
            return Err(Alignment);
        }
        let index = word(block, 20) as usize;
        if word(block, 24) as usize != count || index >= count || numbered[index] {
            return Err(Numbering);
        }
        numbered[index] = true;
        if address < START || address > END - PAYLOAD as u32 {
            return Err(ProtectedMemory);
        }
        blocks.push((address, &block[32..32 + PAYLOAD]));
    }
    blocks.sort_unstable_by_key(|(address, _)| *address);
    if blocks
        .iter()
        .enumerate()
        .any(|(index, (address, _))| *address != START + (index * PAYLOAD) as u32)
    {
        return Err(Coverage);
    }
    if binary.len() < (0x3008 - START) as usize || !binary.len().is_multiple_of(4) {
        return Err(Binary);
    }
    if binary.len() > (END - START) as usize {
        return Err(ExactMatch);
    }
    let padded_size = binary.len().div_ceil(PAGE) * PAGE;
    if count * PAYLOAD != padded_size {
        return Err(ExactMatch);
    }
    for (index, (_, payload)) in blocks.iter().enumerate() {
        let offset = index * PAYLOAD;
        let exact_len = binary.len().saturating_sub(offset).min(PAYLOAD);
        if payload[..exact_len]
            != binary[offset.min(binary.len())..offset.min(binary.len()) + exact_len]
            || payload[exact_len..].iter().any(|byte| *byte != 0xff)
        {
            return Err(ExactMatch);
        }
    }
    let stack_pointer = word(binary, 0);
    if stack_pointer <= 0x20008000 || stack_pointer > 0x20020000 || !stack_pointer.is_multiple_of(8)
    {
        return Err(StackPointer);
    }
    let reset_vector = word(binary, 4);
    let code_address = reset_vector & !1;
    if reset_vector & 1 == 0 || code_address < START || code_address >= START + binary.len() as u32
    {
        return Err(ResetVector);
    }
    if word(binary, 0x200) != 0x87eeb07c {
        return Err(RecoveryMarker);
    }
    if word(binary, (0x3004 - START) as usize) == 0x51b1e5db {
        return Err(OldSoftDevice);
    }
    Ok(ValidatedImage {
        sha256: format!("{:x}", Sha256::digest(image)),
        binary_sha256: format!("{:x}", Sha256::digest(binary)),
        binary_size: binary.len(),
        padded_size,
        stack_pointer,
        reset_vector,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn binary() -> Vec<u8> {
        let mut bytes = vec![0; 0x2008];
        put(&mut bytes, 0, 0x20020000);
        put(&mut bytes, 4, START + 0x101);
        put(&mut bytes, 0x200, 0x87eeb07c);
        bytes
    }

    fn uf2(binary: &[u8]) -> Vec<u8> {
        let mut padded = binary.to_vec();
        padded.resize(binary.len().div_ceil(PAGE) * PAGE, 0xff);
        let count = padded.len() / PAYLOAD;
        let mut image = vec![0; count * 512];
        for (index, block) in image.as_chunks_mut::<512>().0.iter_mut().enumerate() {
            for (offset, value) in [
                (0, 0x0a324655),
                (4, 0x9e5d5157),
                (8, 0x2000),
                (12, START + (index * PAYLOAD) as u32),
                (16, PAYLOAD as u32),
                (20, index as u32),
                (24, count as u32),
                (28, FAMILY),
                (508, 0x0ab16f30),
            ] {
                put(block, offset, value);
            }
            block[32..288].copy_from_slice(&padded[index * PAYLOAD..(index + 1) * PAYLOAD]);
        }
        image
    }

    #[test]
    fn exact_pair_and_reordered_blocks_pass() {
        let bin = binary();
        let mut image = uf2(&bin);
        let proof = validate(&image, &bin).unwrap();
        assert_eq!(proof.binary_size(), 0x2008);
        assert_eq!(proof.start(), START);
        assert_eq!(proof.end_exclusive(), 0x4000);
        assert_eq!(proof.binary_end_exclusive(), 0x3008);
        assert_eq!(proof.blocks(), 48);
        assert_eq!(
            proof.touched_pages().collect::<Vec<_>>(),
            [0x1000, 0x2000, 0x3000]
        );
        // Same synthetic fixture inspected by scripts/migration_guard.py.
        assert_eq!(
            proof.sha256(),
            "fe0101530d7ae5e714cdc09710bbb5a755f82628a63af518bec6da22400fa608"
        );
        assert_eq!(
            proof.binary_sha256(),
            "32d6eb44413acf79c77c3720875d5c1a7fae6f5bfee769b61450d4d71066c3f1"
        );
        image[..1024].rotate_left(512);
        assert!(validate(&image, &bin).is_ok());
    }

    #[test]
    fn rejects_malformed_headers_and_coverage() {
        let bin = binary();
        let original = uf2(&bin);
        for (offset, value, error) in [
            (0, 0, ValidationError::Magic),
            (508, 0, ValidationError::Magic),
            (8, 0x2001, ValidationError::Family),
            (28, 0, ValidationError::Family),
            (16, 128, ValidationError::Alignment),
            (12, START + 1, ValidationError::Alignment),
            (20, 48, ValidationError::Numbering),
            (24, 49, ValidationError::Numbering),
            (12, 0, ValidationError::ProtectedMemory),
            (12, END, ValidationError::ProtectedMemory),
            (512 + 20, 0, ValidationError::Numbering),
            (512 + 12, START, ValidationError::Coverage),
            (512 + 12, START + 512, ValidationError::Coverage),
        ] {
            let mut image = original.clone();
            put(&mut image, offset, value);
            assert_eq!(validate(&image, &bin).unwrap_err(), error, "field {offset}");
        }
        assert_eq!(validate(&[], &bin).unwrap_err(), ValidationError::Blocks);
        assert_eq!(
            validate(&original[..original.len() - 1], &bin).unwrap_err(),
            ValidationError::Blocks
        );
    }

    #[test]
    fn rejects_wrong_bin_padding_vectors_and_markers() {
        let bin = binary();
        let mut image = uf2(&bin);
        image[32 + 8] ^= 1;
        assert_eq!(
            validate(&image, &bin).unwrap_err(),
            ValidationError::ExactMatch
        );
        image = uf2(&bin);
        image[32 + bin.len() / PAYLOAD * 512 + bin.len() % PAYLOAD] = 0;
        assert_eq!(
            validate(&image, &bin).unwrap_err(),
            ValidationError::ExactMatch
        );
        for (offset, value, error) in [
            (0, 0x20008000, ValidationError::StackPointer),
            (0, 0x20020008, ValidationError::StackPointer),
            (0, 0x20010001, ValidationError::StackPointer),
            (4, START + 0x100, ValidationError::ResetVector),
            (4, 1, ValidationError::ResetVector),
            (
                4,
                START + bin.len() as u32 + 1,
                ValidationError::ResetVector,
            ),
            (0x200, 0, ValidationError::RecoveryMarker),
            (0x2004, 0x51b1e5db, ValidationError::OldSoftDevice),
        ] {
            let mut bad = bin.clone();
            put(&mut bad, offset, value);
            assert_eq!(validate(&uf2(&bad), &bad).unwrap_err(), error);
        }
        assert_eq!(
            validate(&uf2(&bin[..0x2004]), &bin[..0x2004]).unwrap_err(),
            ValidationError::Binary
        );
        assert_eq!(
            validate(&image, &bin[..bin.len() - 1]).unwrap_err(),
            ValidationError::Binary
        );
    }

    #[test]
    fn requires_exact_final_page_and_accepts_limit() {
        let mut bin = binary();
        bin.resize((END - START) as usize, 0);
        assert_eq!(validate(&uf2(&bin), &bin).unwrap().end_exclusive(), END);
        bin.extend_from_slice(&[0; 4]);
        assert_eq!(
            validate(&uf2(&bin), &bin).unwrap_err(),
            ValidationError::Blocks
        );
        let bin = binary();
        let mut extra = bin.clone();
        extra.resize(0x4000, 0xff);
        assert_eq!(
            validate(&uf2(&extra), &bin).unwrap_err(),
            ValidationError::ExactMatch
        );
    }
}
