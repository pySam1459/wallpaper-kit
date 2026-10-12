use crate::scene::{InspectionError, PackageSummary, SceneSource};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub(crate) fn asset_key(path: &str) -> Result<String, InspectionError> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.len() > 255
        || path.chars().any(|c| c.is_control() || c == ':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(InspectionError::invalid(
            &path,
            "asset path must be relative and contain no parent, empty, drive or stream components",
        ));
    }
    Ok(path)
}

#[derive(Clone, Copy)]
struct Entry {
    offset: u64,
    length: u64,
}

pub(crate) struct Package {
    _guard: AssetReader,
    path: PathBuf,
    engine: PathBuf,
    entries: BTreeMap<String, Entry>,
    pub(crate) summary: PackageSummary,
}

impl Package {
    pub(crate) fn open(source: &SceneSource) -> Result<Self, InspectionError> {
        let path = canonical(&source.package)?;
        let engine = canonical(&source.engine_assets)?;
        if !engine.is_dir() {
            return Err(InspectionError::invalid(
                &engine.display().to_string(),
                "engine asset root must be a directory",
            ));
        }
        let mut reader = AssetReader::file(&path)?;
        let signature = reader.string(32)?;
        let version = signature
            .strip_prefix("PKGV")
            .filter(|suffix| suffix.len() == 4 && suffix.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|suffix| suffix.parse::<u32>().ok());
        if !version.is_some_and(|version| (1..=24).contains(&version)) {
            return Err(reader.invalid(format!("unsupported package signature {signature}")));
        }
        let count = reader.count(65_536, "package entries")?;
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let key = asset_key(&reader.string(255)?)?;
            let entry = Entry {
                offset: u64::from(reader.u32()?),
                length: u64::from(reader.u32()?),
            };
            if entries.insert(key.to_ascii_lowercase(), entry).is_some() {
                return Err(reader.invalid(format!("duplicate package path {key}")));
            }
            if reader.position > 16 * 1024 * 1024 {
                return Err(reader.invalid("package index exceeds 16 MiB"));
            }
        }
        let data_start = reader.position;
        let mut ranges = Vec::new();
        for entry in entries.values_mut() {
            entry.offset += data_start;
            let end = entry
                .offset
                .checked_add(entry.length)
                .ok_or_else(|| reader.invalid("package range overflow"))?;
            if end > reader.length {
                return Err(reader.invalid("package entry extends beyond file"));
            }
            if entry.length > 0 {
                ranges.push((entry.offset, end));
            }
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(reader.invalid("overlapping package entries"));
        }
        let bytes = reader.length;
        let sha256 = AssetReader::file(&path)?.hash()?;
        Ok(Self {
            _guard: reader,
            path: path.clone(),
            engine,
            entries,
            summary: PackageSummary {
                path,
                signature,
                bytes,
                sha256,
                entries: count as usize,
            },
        })
    }

    pub(crate) fn reader(&self, key: &str) -> Result<Option<AssetReader>, InspectionError> {
        let key = asset_key(key)?;
        if let Some(entry) = self.entries.get(&key.to_ascii_lowercase()) {
            let mut reader = AssetReader::file(&self.path)?;
            reader
                .file
                .seek(SeekFrom::Start(entry.offset))
                .map_err(|error| InspectionError::io(&key, error))?;
            reader.asset = key;
            reader.start = entry.offset;
            reader.length = entry.length;
            reader.origin = "package";
            return Ok(Some(reader));
        }
        let requested = self.engine.join(&key);
        let resolved = match requested.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(InspectionError::io(&key, error)),
        };
        if !resolved.starts_with(&self.engine) {
            return Err(InspectionError::invalid(
                &key,
                "resolved asset escapes the engine root",
            ));
        }
        let mut reader = AssetReader::file(&resolved)?;
        reader.asset = key;
        reader.origin = "engine";
        Ok(Some(reader))
    }

    pub(crate) fn scene_key(&self) -> &'static str {
        if !self.entries.contains_key("scene.json") && self.entries.contains_key("gifscene.json") {
            "gifscene.json"
        } else {
            "scene.json"
        }
    }
}

fn canonical(path: &Path) -> Result<PathBuf, InspectionError> {
    path.canonicalize()
        .map_err(|error| InspectionError::io(&path.display().to_string(), error))
}

/// A file-backed, range-checked view of one asset.
pub(crate) struct AssetReader {
    file: File,
    start: u64,
    pub(crate) position: u64,
    pub(crate) length: u64,
    pub(crate) asset: String,
    pub(crate) origin: &'static str,
}

impl AssetReader {
    fn file(path: &Path) -> Result<Self, InspectionError> {
        let asset = path.display().to_string();
        // FILE_SHARE_READ permits inspection handles while excluding writes and
        // replacement during a read. The package guard lives through traversal.
        let file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(path)
            .map_err(|error| InspectionError::io(&asset, error))?;
        let metadata = file
            .metadata()
            .map_err(|error| InspectionError::io(&asset, error))?;
        if !metadata.is_file() {
            return Err(InspectionError::invalid(
                &asset,
                "asset must be a regular file",
            ));
        }
        Ok(Self {
            file,
            start: 0,
            position: 0,
            length: metadata.len(),
            asset,
            origin: "engine",
        })
    }

    pub(crate) fn invalid(&self, detail: impl Into<String>) -> InspectionError {
        InspectionError::invalid(&self.asset, detail)
    }

    pub(crate) fn read<const N: usize>(&mut self) -> Result<[u8; N], InspectionError> {
        let mut bytes = [0; N];
        self.read_into(&mut bytes)?;
        Ok(bytes)
    }

    fn read_into(&mut self, bytes: &mut [u8]) -> Result<(), InspectionError> {
        if bytes.len() as u64 > self.length - self.position {
            return Err(self.invalid("truncated asset or out-of-range read"));
        }
        self.file
            .read_exact(bytes)
            .map_err(|error| InspectionError::io(&self.asset, error))?;
        self.position += bytes.len() as u64;
        Ok(())
    }

    pub(crate) fn u32(&mut self) -> Result<u32, InspectionError> {
        Ok(u32::from_le_bytes(self.read()?))
    }
    pub(crate) fn f32(&mut self) -> Result<f32, InspectionError> {
        Ok(f32::from_le_bytes(self.read()?))
    }

    pub(crate) fn count(&mut self, maximum: u32, name: &str) -> Result<u32, InspectionError> {
        let count = self.u32()?;
        if count > maximum {
            return Err(self.invalid(format!("{name} exceeds {maximum}")));
        }
        Ok(count)
    }

    pub(crate) fn bytes(&mut self, length: u64, maximum: u64) -> Result<Vec<u8>, InspectionError> {
        if length > maximum || length > self.length - self.position {
            return Err(self.invalid("asset allocation or range limit exceeded"));
        }
        let mut bytes = vec![0; length as usize];
        self.read_into(&mut bytes)?;
        Ok(bytes)
    }

    pub(crate) fn copy_to(
        &mut self,
        output: &mut impl Write,
        length: u64,
        maximum: u64,
    ) -> Result<(), InspectionError> {
        if length > maximum || length > self.length - self.position {
            return Err(self.invalid("asset copy range or size limit exceeded"));
        }
        let mut remaining = length;
        let mut buffer = [0; 65536];
        while remaining > 0 {
            let count = remaining.min(buffer.len() as u64) as usize;
            self.read_into(&mut buffer[..count])?;
            output
                .write_all(&buffer[..count])
                .map_err(|error| InspectionError::io(&self.asset, error))?;
            remaining -= count as u64;
        }
        Ok(())
    }

    fn string(&mut self, maximum: u64) -> Result<String, InspectionError> {
        let length = u64::from(self.u32()?);
        String::from_utf8(self.bytes(length, maximum)?)
            .map_err(|_| self.invalid("invalid UTF-8 package path or signature"))
    }

    pub(crate) fn signature(&mut self) -> Result<String, InspectionError> {
        let bytes = self.read::<9>()?;
        if bytes[8] != 0 {
            return Err(self.invalid("unterminated TEX signature"));
        }
        String::from_utf8(bytes[..8].to_vec()).map_err(|_| self.invalid("invalid TEX signature"))
    }

    pub(crate) fn skip(&mut self, length: u64) -> Result<(), InspectionError> {
        if length > self.length - self.position {
            return Err(self.invalid("payload extends beyond asset"));
        }
        self.position += length;
        self.file
            .seek(SeekFrom::Start(self.start + self.position))
            .map_err(|error| InspectionError::io(&self.asset, error))?;
        Ok(())
    }

    pub(crate) fn seek_to(&mut self, position: u64) -> Result<(), InspectionError> {
        if position > self.length {
            return Err(self.invalid("seek extends beyond asset"));
        }
        self.file
            .seek(SeekFrom::Start(self.start + position))
            .map_err(|error| InspectionError::io(&self.asset, error))?;
        self.position = position;
        Ok(())
    }

    pub(crate) fn hash(&mut self) -> Result<String, InspectionError> {
        self.position = 0;
        self.file
            .seek(SeekFrom::Start(self.start))
            .map_err(|error| InspectionError::io(&self.asset, error))?;
        let mut digest = Sha256::new();
        let mut buffer = [0; 65_536];
        while self.position < self.length {
            let length = (self.length - self.position).min(buffer.len() as u64) as usize;
            self.read_into(&mut buffer[..length])?;
            digest.update(&buffer[..length]);
        }
        self.position = 0;
        self.file
            .seek(SeekFrom::Start(self.start))
            .map_err(|error| InspectionError::io(&self.asset, error))?;
        Ok(format!("{:x}", digest.finalize()))
    }
}
