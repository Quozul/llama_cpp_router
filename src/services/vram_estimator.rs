use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

/// ---------------------------------------------------------------------------
/// Public API
/// ---------------------------------------------------------------------------

/// Result of the memory‑estimation algorithm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEstimation {
    /// Model file size in megabytes (rounded down).
    pub model_size_mb: u64,
    /// KV‑cache size in megabytes (rounded down).
    pub kv_cache_mb: u64,
    /// Total RAM required in megabytes (rounded down).
    pub total_required_mb: u64,
    /// Human‑readable string, e.g. `3.1 GB (Model: 2.6 GB + KV: 500 MB @ Q4)`.
    pub display: String,
    /// Which quantisation was used for the KV‑cache.
    pub kv_quant: KvQuant,
}

/// Supported KV‑cache quantisations (the same set that the original HTML page
/// exposes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvQuant {
    FP32,
    FP16,
    Int8,
    Q6,
    Q5,
    Q4,
}

impl KvQuant {
    /// Bytes (actually “bytes per value”) used by the selected KV‑quant.
    fn bytes_per_value(self) -> f64 {
        match self {
            KvQuant::FP32 => 8.0,
            KvQuant::FP16 => 4.0,
            KvQuant::Int8 => 2.0,
            KvQuant::Q6 => 1.5,
            KvQuant::Q5 => 1.25,
            KvQuant::Q4 => 1.0,
        }
    }

    fn label(self) -> &'static str {
        match self {
            KvQuant::FP32 => "FP32",
            KvQuant::FP16 => "FP16/BF16",
            KvQuant::Int8 => "INT8",
            KvQuant::Q6 => "Q6 (6‑bit)",
            KvQuant::Q5 => "Q5 (5‑bit)",
            KvQuant::Q4 => "Q4 (4‑bit)",
        }
    }
}

/// Estimate the RAM required to run a model that is stored in a GGUF file.
///
/// * `path` – local path to the **first** shard of the model (or the complete
///   file if it is not sharded).
/// * `context_tokens` – number of tokens the KV‑cache must be able to hold.
/// * `kv_quant` – quantisation that the runtime will use for the KV‑cache.
///
/// The function does **not** download anything; it only reads the file (or the
/// first shard) and uses the file size to compute the model size.  If the file
/// appears to be part of a sharded model (its name matches the pattern
/// `-NNN-of-MMM.*`), the size is multiplied by the total number of shards
/// reported in the metadata (or inferred from the filename) to obtain a better
/// estimate.
///
/// Returns `None` when the needed metadata cannot be located, otherwise a
/// `MemoryEstimation`.
pub fn estimate_memory<P: AsRef<Path>>(
    path: P,
    context_tokens: usize,
    kv_quant: KvQuant,
) -> io::Result<Option<MemoryEstimation>> {
    // -----------------------------------------------------------------------
    // 1) Open file + read the important GGUF metadata.
    // -----------------------------------------------------------------------
    let file = File::open(&path)?;
    let mut src = BufReader::new(file);
    let params = read_model_params(&mut src)?;

    // -----------------------------------------------------------------------
    // 2) Resolve the *total* model size in bytes.
    //    If the file is a shard we try to guess the total size.
    // -----------------------------------------------------------------------
    let file_size = src.get_ref().metadata()?.len();

    let total_bytes = if let Some(split_cnt) = params.split_count.filter(|&c| c > 1) {
        // Prefer the split count from metadata; fall back to the number that can be
        // inferred from the filename.
        let inferred_from_name = infer_split_count_from_path(path.as_ref())?;
        let shards = inferred_from_name.unwrap_or(split_cnt);
        file_size.saturating_mul(shards as u64)
    } else {
        file_size
    };

    // -----------------------------------------------------------------------
    // 3) Compute the memory consumption.
    // -----------------------------------------------------------------------
    let model_mb = total_bytes / 1_000_000;
    let kv_bytes = kv_quant.bytes_per_value()
        * params.hidden_size.unwrap() as f64
        * params.hidden_layers.unwrap() as f64
        * context_tokens as f64;
    let kv_mb = (kv_bytes / 1_000_000.0).floor() as u64;
    let total_mb = model_mb + kv_mb;

    // -----------------------------------------------------------------------
    // 4) Build the display string.
    // -----------------------------------------------------------------------
    let display = format!(
        "{} (Model: {} + KV: {} @ {})",
        fmt_memory(total_mb),
        fmt_memory(model_mb),
        fmt_memory(kv_mb),
        kv_quant.label()
    );

    Ok(Some(MemoryEstimation {
        model_size_mb: model_mb,
        kv_cache_mb: kv_mb,
        total_required_mb: total_mb,
        display,
        kv_quant,
    }))
}

/// ---------------------------------------------------------------------------
/// Helper data structures & functions
/// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct ModelParams {
    attention_heads: Option<u32>,
    kv_heads: Option<u32>,
    hidden_layers: Option<u32>,
    hidden_size: Option<u64>,
    split_count: Option<u32>,
}

/// Reads the GGUF header and extracts only the parameters we need.
fn read_model_params<R: Read + Seek>(src: &mut R) -> io::Result<ModelParams> {
    // 1) Magic number
    let magic = read_u32(src)?;
    if magic != 0x4655_4747 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid GGUF magic: 0x{:08x}", magic),
        ));
    }

    // 2) Version (currently we accept any version ≤ 3)
    let version = read_u32(src)?;
    if version > 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unsupported GGUF version: {}", version),
        ));
    }

    // 3) Tensor count (skip for version ≥ 1)
    if version >= 1 {
        _ = read_u64(src)?;
    }

    // 4) Metadata count
    let meta_cnt = read_u64(src)?;

    // Suffixes we are interested in
    const SUFFIXES: [&str; 5] = [
        ".attention.head_count",
        ".attention.head_count_kv",
        ".block_count",
        ".embedding_length",
        "split.count",
    ];

    let mut params = ModelParams::default();

    for _ in 0..meta_cnt {
        let key = read_string(src)?;
        let typ = read_u32(src)?;
        // -------------------------------------------------------------------
        // Decide whether we need the value or just skip it.
        // -------------------------------------------------------------------
        let need = SUFFIXES.iter().any(|suf| key.ends_with(suf));
        if need {
            match key.as_str() {
                k if k.ends_with(".attention.head_count") => {
                    let val = read_u32_of_type(src, typ)?;
                    params.attention_heads = Some(val);
                }
                k if k.ends_with(".attention.head_count_kv") => {
                    let val = read_u32_of_type(src, typ)?;
                    params.kv_heads = Some(val);
                }
                k if k.ends_with(".block_count") => {
                    let val = read_u32_of_type(src, typ)?;
                    params.hidden_layers = Some(val);
                }
                k if k.ends_with(".embedding_length") => {
                    let val = read_u64_of_type(src, typ)?;
                    params.hidden_size = Some(val);
                }
                k if k.ends_with("split.count") => {
                    let val = read_u32_of_type(src, typ)?;
                    params.split_count = Some(val);
                }
                _ => {
                    // Should never get here because of the `need` guard.
                    skip_value(src, typ)?;
                }
            }
        } else {
            // Not a key we care about → just skip the value.
            skip_value(src, typ)?;
        }

        // Early exit when everything we need has been found.
        if params.attention_heads.is_some()
            && params.hidden_layers.is_some()
            && params.hidden_size.is_some()
        {
            // kv_heads is optional – if missing we later copy attention_heads.
            break;
        }
    }

    // If the model does not store `kv_heads` we fall back to `attention_heads`.
    if params.kv_heads.is_none() {
        params.kv_heads = params.attention_heads;
    }

    // Make sure the required fields are present.
    if params.attention_heads.is_none()
        || params.hidden_layers.is_none()
        || params.hidden_size.is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Could not locate all required model parameters",
        ));
    }

    Ok(params)
}

/// ---------------------------------------------------------------------------
/// Binary reading utilities (little‑endian)
/// ---------------------------------------------------------------------------
fn read_u8<R: Read>(src: &mut R) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    src.read_exact(&mut buf)?;
    Ok(buf[0])
}
fn read_u16<R: Read>(src: &mut R) -> io::Result<u16> {
    let mut buf = [0u8; 2];
    src.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}
fn read_u32<R: Read>(src: &mut R) -> io::Result<u32> {
    let mut buf = [0u8; 4];
    src.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}
fn read_u64<R: Read>(src: &mut R) -> io::Result<u64> {
    let mut buf = [0u8; 8];
    src.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Reads a UTF‑8 string: length (u64) + bytes.
fn read_string<R: Read + Seek>(src: &mut R) -> io::Result<String> {
    let len = read_u64(src)?;
    if len > 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("String length too large: {}", len),
        ));
    }
    let mut buf = vec![0u8; len as usize];
    src.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// ---------------------------------------------------------------------------
/// Value reading helpers for the few primitive types we care about
/// ---------------------------------------------------------------------------
fn read_u32_of_type<R: Read>(src: &mut R, typ: u32) -> io::Result<u32> {
    match typ {
        0 => Ok(read_u8(src)? as u32),  // UINT8
        1 => Ok(read_u8(src)? as u32),  // INT8
        2 => Ok(read_u16(src)? as u32), // UINT16
        3 => Ok(read_u16(src)? as u32), // INT16
        4 => read_u32(src),             // UINT32
        5 => read_u32(src),             // INT32
        6 => {
            // FLOAT32 – we treat it as u32 bits
            read_u32(src)
        }
        8 => {
            // STRING – not a numeric value, treat as error
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unexpected string where a numeric value was expected",
            ))
        }
        10 => {
            // UINT64 – may not fit in u32, cap to u32
            let v = read_u64(src)?;
            if v > u32::MAX as u64 {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "UINT64 value does not fit into u32",
                ))
            } else {
                Ok(v as u32)
            }
        }
        11 => {
            // INT64 – same handling as UINT64
            let v = read_u64(src)?;
            if v > u32::MAX as u64 {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "INT64 value does not fit into u32",
                ))
            } else {
                Ok(v as u32)
            }
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unsupported type {} for u32 value", typ),
        )),
    }
}

fn read_u64_of_type<R: Read + Seek>(src: &mut R, typ: u32) -> io::Result<u64> {
    match typ {
        4 => Ok(read_u32(src)? as u64),
        5 => Ok(read_u32(src)? as u64),
        10 => read_u64(src),
        11 => read_u64(src),
        8 => {
            // STRING – we interpret it as a UTF‑8 number for convenience
            let s = read_string(src)?;
            s.trim()
                .parse::<u64>()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unsupported type {} for u64 value", typ),
        )),
    }
}

/// Skip a GGUF value whose exact binary format we do not care about.
fn skip_value<R: Read + Seek>(src: &mut R, typ: u32) -> io::Result<()> {
    match typ {
        0 | 1 => src.seek(SeekFrom::Current(1)).map(|_| ()), // UINT8 / INT8
        2 | 3 => src.seek(SeekFrom::Current(2)).map(|_| ()), // UINT16 / INT16
        4..=6 => src.seek(SeekFrom::Current(4)).map(|_| ()), // UINT32 / INT32 / FLOAT32
        7 => src.seek(SeekFrom::Current(1)).map(|_| ()),     // BOOL
        8 => {
            // STRING: read length then seek that many bytes
            let len = read_u64(src)?;
            if len > 10_000_000 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "String length suspiciously large while skipping",
                ));
            }
            src.seek(SeekFrom::Current(len as i64)).map(|_| ())
        }
        9 => {
            // ARRAY: element type + count + elements
            let elem_type = read_u32(src)?;
            let count = read_u64(src)?;
            for _ in 0..count {
                skip_value(src, elem_type)?;
            }
            Ok(())
        }
        10..=12 => src.seek(SeekFrom::Current(8)).map(|_| ()), // UINT64 / INT64 / FLOAT64
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unknown GGUF type {}", typ),
        )),
    }
}

/// ---------------------------------------------------------------------------
/// Helper to infer split‑count from a filename like “…-001-of-005.gguf”.
/// ---------------------------------------------------------------------------
fn infer_split_count_from_path(path: &Path) -> io::Result<Option<u32>> {
    let name = match path.file_name().and_then(|s| s.to_str()) {
        Some(n) => n,
        None => return Ok(None),
    };
    // Look for “-NNN-of-MMM” (where N and M are any number of digits, same width)
    let re =
        regex::Regex::new(r"-(\d+)-of-(\d+)$").expect("hard‑coded regex should always compile");
    if let Some(caps) = re.captures(name) {
        let total: u32 = caps[2].parse().unwrap_or(0);
        if total > 1 {
            return Ok(Some(total));
        }
    }
    Ok(None)
}

/// ---------------------------------------------------------------------------
/// Human‑readable memory formatting (the same behaviour as the HTML page).
/// ---------------------------------------------------------------------------
fn fmt_memory(mb: u64) -> String {
    if mb >= 1000 {
        format!("{:.1} GB", mb as f64 / 1000.0)
    } else {
        format!("{} MB", mb)
    }
}
