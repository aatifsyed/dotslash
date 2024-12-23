/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::Context as _;
use schemars::gen::SchemaGenerator;
use schemars::schema::Schema;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_jsonrc::value::Value;

use crate::artifact_path::ArtifactPath;
use crate::digest::Digest;
use crate::fetch_method::ArtifactFormat;

/// A DotSlash file must start with exactly these bytes on the first line
/// to be considered valid. Because a DotSlash file does not have a
/// standard extension, this gives us a reliable way to identify
/// all of the DotSlash files in the repo.
pub const REQUIRED_HEADER: &str = "#!/usr/bin/env dotslash";

#[derive(Deserialize, Debug, PartialEq, JsonSchema)]
pub struct ConfigFile {
    pub name: String,
    pub platforms: HashMap<String, ArtifactEntry>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub struct ArtifactEntry<Format = ArtifactFormat> {
    pub size: u64,
    pub hash: HashAlgorithm,
    pub digest: Digest,
    #[serde(default)]
    pub format: Format,
    pub path: ArtifactPath,
    pub providers: Vec<Value>,
    #[serde(default = "readonly_default_as_true", skip_serializing_if = "is_true")]
    pub readonly: bool,
}

impl JsonSchema for ArtifactEntry {
    fn schema_name() -> String {
        String::from("ArtifactEntry")
    }
    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        pub struct _ArtifactEntry {
            pub size: u64,
            pub hash: HashAlgorithm,
            pub digest: Digest,
            #[serde(default)]
            pub format: ArtifactFormat,
            pub path: ArtifactPath,
            #[schemars(with = "Vec<serde_json::Value>")]
            pub providers: Vec<Value>,
            #[serde(default = "readonly_default_as_true", skip_serializing_if = "is_true")]
            pub readonly: bool,
        }
        _ArtifactEntry::json_schema(gen)
    }
    fn schema_id() -> Cow<'static, str> {
        Cow::Borrowed(std::concat!(std::module_path!(), "::", "ArtifactEntry"))
    }
}

/// While having a boolean that defaults to `true` is somewhat undesirable,
/// the alternative would be to name the field "writable", which is too easy
/// to misspell as "writeable" (which would be ignored), so "readonly" it is.
fn readonly_default_as_true() -> bool {
    true
}

#[expect(clippy::trivially_copy_pass_by_ref)]
fn is_true(b: &bool) -> bool {
    *b
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, JsonSchema)]
pub enum HashAlgorithm {
    #[serde(rename = "blake3")]
    Blake3,
    #[serde(rename = "sha256")]
    Sha256,
}

pub fn parse_file(data: &str) -> anyhow::Result<(Value, ConfigFile)> {
    // Check to see whether the DotSlash file starts with the proper shebang.
    let data = data
        .strip_prefix(REQUIRED_HEADER)
        .and_then(|rest| {
            rest.strip_prefix("\r\n")
                .or_else(|| rest.strip_prefix('\n'))
        })
        .with_context(|| {
            anyhow::format_err!("DotSlash file must start with `{REQUIRED_HEADER}`")
        })?;

    let value = serde_jsonrc::from_str::<Value>(data)?;
    let config_file = ConfigFile::deserialize(&value)?;
    Ok((value, config_file))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::config::ArtifactPath;
    use crate::fetch_method::ArtifactFormat;

    #[test]
    fn json_schema() {
        let schema = schemars::schema_for!(ConfigFile);
        let expected = serde_json::to_string_pretty(&schema).unwrap();
        expect_test::expect_file!["schema.json"].assert_eq(&expected);
    }

    fn parse_file_string(json: &str) -> anyhow::Result<ConfigFile> {
        Ok(parse_file(json)?.1)
    }

    #[test]
    fn extract_config_file() {
        let dotslash = r#"#!/usr/bin/env dotslash
        {
            "name": "my_tool",
            "platforms": {
                "linux-x86_64": {
                    "size": 123,
                    "hash": "sha256",
                    "digest": "7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069",
                    "format": "tar",
                    "path": "bindir/my_tool",
                    "providers": [
                        {
                            "type": "http",
                            "url": "https://example.com/my_tool.tar"
                        }
                    ]
                },
            },
        }
        "#;
        let config_file = parse_file_string(dotslash).unwrap();
        assert_eq!(
            config_file,
            ConfigFile {
                name: "my_tool".to_owned(),
                platforms: [(
                    "linux-x86_64".to_owned(),
                    ArtifactEntry {
                        size: 123,
                        hash: HashAlgorithm::Sha256,
                        digest: Digest::try_from(
                            "7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069"
                                .to_owned(),
                        )
                        .unwrap(),
                        format: ArtifactFormat::Tar,
                        path: ArtifactPath::from_str("bindir/my_tool").unwrap(),
                        providers: vec![serde_jsonrc::json!({
                            "type": "http",
                            "url": "https://example.com/my_tool.tar",
                        })],
                        readonly: true,
                    }
                )]
                .into(),
            },
        );
    }

    #[test]
    fn rename() {
        let dotslash = r#"#!/usr/bin/env dotslash
        {
            "name": "minesweeper",
            "platforms": {
                "linux-x86_64": {
                    "size": 123,
                    "hash": "sha256",
                    "digest": "7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069",
                    "path": "minesweeper.exe",
                    "providers": [
                        {
                            "type": "http",
                            "url": "https://foo.com"
                        }
                    ],
                },
            },
        }
        "#;
        let config_file = parse_file_string(dotslash).unwrap();
        assert_eq!(
            config_file,
            ConfigFile {
                name: "minesweeper".to_owned(),
                platforms: [(
                    "linux-x86_64".to_owned(),
                    ArtifactEntry {
                        size: 123,
                        hash: HashAlgorithm::Sha256,
                        digest: Digest::try_from(
                            "7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069"
                                .to_owned(),
                        )
                        .unwrap(),
                        format: ArtifactFormat::Plain,
                        path: ArtifactPath::from_str("minesweeper.exe").unwrap(),
                        providers: vec![serde_jsonrc::json!({
                            "type": "http",
                            "url": "https://foo.com",
                        })],
                        readonly: true,
                    }
                )]
                .into(),
            }
        );
    }

    #[test]
    fn header_must_be_present() {
        let dotslash = r#"
        {
            "name": "made-up",
            "platforms": {
            },
        }
        "#;
        assert_eq!(
            parse_file_string(dotslash).map_err(|x| x.to_string()),
            Err("DotSlash file must start with `#!/usr/bin/env dotslash`".to_owned()),
        );
    }
}
