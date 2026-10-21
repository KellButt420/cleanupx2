//! File-type categories used by "group by type".

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Images,
    Videos,
    Audio,
    Documents,
    Archives,
    Installers,
    Other,
}

impl Category {
    pub fn folder_name(self) -> &'static str {
        match self {
            Category::Images => "Images",
            Category::Videos => "Videos",
            Category::Audio => "Audio",
            Category::Documents => "Documents",
            Category::Archives => "Archives",
            Category::Installers => "Installers",
            Category::Other => "Other",
        }
    }
}

/// Category for a lower-case extension without the dot.
pub fn categorize(extension: &str) -> Category {
    match extension {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "heic" | "heif" | "tif" | "tiff"
        | "svg" | "ico" | "raw" | "cr2" | "nef" | "arw" | "dng" | "avif" | "psd" => {
            Category::Images
        }
        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "flv" | "mpg" | "mpeg" | "3gp" => {
            Category::Videos
        }
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" | "mid"
        | "midi" => Category::Audio,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp"
        | "rtf" | "txt" | "md" | "csv" | "epub" | "pages" | "numbers" | "key" | "xps" | "one" => {
            Category::Documents
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "iso" | "cab" => {
            Category::Archives
        }
        "exe" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" | "msp" => {
            Category::Installers
        }
        _ => Category::Other,
    }
}

/// The simple four-folder scheme: documents, media, binaries, and everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleGroup {
    Doc,
    Media,
    Binary,
    Others,
}

impl SimpleGroup {
    pub fn folder_name(self) -> &'static str {
        match self {
            SimpleGroup::Doc => "Doc",
            SimpleGroup::Media => "Media",
            SimpleGroup::Binary => "Binary",
            SimpleGroup::Others => "Others",
        }
    }
}

/// Doc: documents and text (.pdf, .doc, .txt, …). Media: images, audio, and
/// video (.jpeg, .mp3, .gif, …). Binary: programs, libraries, archives, and
/// disk images (.exe, .zip, .dll, …). Everything else goes to Others.
pub fn simple_group(extension: &str) -> SimpleGroup {
    match categorize(extension) {
        Category::Documents => return SimpleGroup::Doc,
        Category::Images | Category::Videos | Category::Audio => return SimpleGroup::Media,
        Category::Archives | Category::Installers => return SimpleGroup::Binary,
        Category::Other => {}
    }
    match extension {
        "log" | "json" | "xml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "toml" | "html"
        | "htm" | "tex" | "odg" | "wpd" | "wps" | "msg" | "eml" | "vcf" | "ics" | "srt" | "vtt" => {
            SimpleGroup::Doc
        }
        "dll" | "sys" | "bin" | "dat" | "jar" | "apk" | "xapk" | "dmg" | "deb" | "rpm" | "img"
        | "vhd" | "vhdx" | "lib" | "so" | "dylib" | "ocx" | "com" | "scr" | "efi" | "pak"
        | "lzma" | "z" | "arj" | "lz" | "lz4" | "wim" | "esd" => SimpleGroup::Binary,
        _ => SimpleGroup::Others,
    }
}

/// Extensions of files that are still being written by a download manager.
pub fn is_partial_download(extension: &str) -> bool {
    matches!(
        extension,
        "crdownload" | "part" | "partial" | "download" | "opdownload" | "!ut" | "tmp"
    )
}

pub fn is_shortcut(extension: &str) -> bool {
    matches!(extension, "lnk" | "url" | "appref-ms")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_scheme_matches_the_documented_examples() {
        for ext in ["pdf", "doc", "txt"] {
            assert_eq!(simple_group(ext), SimpleGroup::Doc, "{ext}");
        }
        for ext in ["jpeg", "mp3", "gif", "mp4"] {
            assert_eq!(simple_group(ext), SimpleGroup::Media, "{ext}");
        }
        for ext in ["exe", "zip", "dll", "msi", "iso"] {
            assert_eq!(simple_group(ext), SimpleGroup::Binary, "{ext}");
        }
        for ext in ["xyz", "", "blend"] {
            assert_eq!(simple_group(ext), SimpleGroup::Others, "{ext:?}");
        }
    }

    #[test]
    fn common_types_are_categorized() {
        assert_eq!(categorize("png"), Category::Images);
        assert_eq!(categorize("mkv"), Category::Videos);
        assert_eq!(categorize("flac"), Category::Audio);
        assert_eq!(categorize("pdf"), Category::Documents);
        assert_eq!(categorize("7z"), Category::Archives);
        assert_eq!(categorize("msi"), Category::Installers);
        assert_eq!(categorize("xyz"), Category::Other);
        assert!(is_partial_download("crdownload"));
        assert!(is_shortcut("lnk"));
    }
}
