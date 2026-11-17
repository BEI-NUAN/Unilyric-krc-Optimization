//! KRC 格式生成器

use std::fmt::Write;

use lyrics_helper_core::{ContentType, ConvertError, LyricLine, MetadataStore};

/// KRC 生成的主入口函数。
pub fn generate_krc(
    lines: &[LyricLine],
    metadata_store: &MetadataStore,
) -> Result<String, ConvertError> {
    let mut krc_output = String::new();

    writeln!(krc_output, "{}", metadata_store.generate_lrc_header())?;

    for line in lines {
        // KRC 不支持背景人声
        if let Some(main_track) = line
            .tracks
            .iter()
            .find(|t| t.content_type == ContentType::Main)
        {
            let syllables: Vec<_> = main_track
                .content
                .words
                .iter()
                .flat_map(|w| &w.syllables)
                .collect();

            if syllables.is_empty() {
                continue;
            }

            let line_duration = line.end_ms.saturating_sub(line.start_ms);
            write!(krc_output, "[{},{}]", line.start_ms, line_duration)?;

            for syl in syllables {
                let offset_ms = syl.start_ms.saturating_sub(line.start_ms);
                let duration_ms = syl.end_ms.saturating_sub(syl.start_ms);

                write!(krc_output, "<{},{},0>{}", offset_ms, duration_ms, syl.text)?;

                // `LyricSyllable::text` 不包含空格，音节之间是否有空格由
                // `ends_with_space` 记录，必须在这里序列化出去，否则 TTML 中
                // 相邻 `<span>` 之间的空格会在转换为 KRC 时丢失。
                if syl.ends_with_space {
                    krc_output.push(' ');
                }
            }

            writeln!(krc_output)?;
        }
    }

    Ok(krc_output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyrics_helper_core::{AnnotatedTrack, LyricSyllable, LyricTrack, Word};

    /// 构造一个音节。`end_ms` 需大于等于 `start_ms`。
    fn syl(text: &str, start_ms: u64, end_ms: u64, ends_with_space: bool) -> LyricSyllable {
        LyricSyllable {
            text: text.to_string(),
            start_ms,
            end_ms,
            duration_ms: Some(end_ms.saturating_sub(start_ms)),
            ends_with_space,
        }
    }

    /// 构造只包含一条主歌词轨道的单行歌词。
    fn make_line(start_ms: u64, end_ms: u64, syllables: Vec<LyricSyllable>) -> LyricLine {
        LyricLine {
            tracks: vec![AnnotatedTrack {
                content_type: ContentType::Main,
                content: LyricTrack {
                    words: vec![Word {
                        syllables,
                        furigana: None,
                    }],
                    metadata: std::collections::HashMap::new(),
                },
                translations: Vec::new(),
                romanizations: Vec::new(),
            }],
            start_ms,
            end_ms,
            ..Default::default()
        }
    }

    /// 取出输出中的歌词行（跳过文件头与空行）。
    fn lyric_line(output: &str) -> &str {
        output
            .lines()
            .find(|line| line.starts_with('['))
            .expect("生成的 KRC 中应包含歌词行")
    }

    /// 提取所有音节时间标签的内容（`offset,duration,0`），用于比较时间轴。
    fn syllable_timestamps(output: &str) -> Vec<&str> {
        output
            .split('<')
            .skip(1)
            .filter_map(|segment| {
                let tag = segment.split('>').next()?;
                tag.contains(',').then_some(tag)
            })
            .collect()
    }

    // 测试 A：`ends_with_space == true` 的音节后必须输出一个空格。
    #[test]
    fn test_preserves_space_after_syllable() {
        let lines = vec![make_line(
            0,
            200,
            vec![syl("探", 0, 100, true), syl("测", 100, 200, false)],
        )];

        let output = generate_krc(&lines, &MetadataStore::new()).unwrap();

        assert_eq!(lyric_line(&output), "[0,200]<0,100,0>探 <100,100,0>测");
    }

    // 测试 B：所有 `ends_with_space == false` 时不得擅自添加空格。
    #[test]
    fn test_does_not_add_space_when_flag_is_false() {
        let lines = vec![make_line(
            0,
            200,
            vec![syl("探", 0, 100, false), syl("测", 100, 200, false)],
        )];

        let output = generate_krc(&lines, &MetadataStore::new()).unwrap();

        assert_eq!(lyric_line(&output), "[0,200]<0,100,0>探<100,100,0>测");
    }

    // 测试 C：连续多个音节的空格标记只出现在对应音节之后，不丢失、不重复、不位移。
    #[test]
    fn test_spaces_follow_marked_syllables_only() {
        let lines = vec![make_line(
            0,
            400,
            vec![
                syl("A", 0, 100, true),
                syl("B", 100, 200, true),
                syl("C", 200, 300, false),
                syl("D", 300, 400, true),
            ],
        )];

        let output = generate_krc(&lines, &MetadataStore::new()).unwrap();

        assert_eq!(
            lyric_line(&output),
            "[0,400]<0,100,0>A <100,100,0>B <200,100,0>C<300,100,0>D "
        );
    }

    // 测试 D：空格输出逻辑不得改变任何 offset / duration 数值。
    #[test]
    fn test_timeline_is_unchanged_by_space_flag() {
        // 行从 500ms 开始，音节之间留有间隙，用于同时覆盖 offset 与 duration 计算。
        let syllables_without_space = vec![
            syl("探", 500, 800, false),
            syl("测", 900, 1300, false),
            syl("行", 1300, 1500, false),
        ];
        let syllables_with_space = vec![
            syl("探", 500, 800, true),
            syl("测", 900, 1300, true),
            syl("行", 1300, 1500, false),
        ];

        let output_without_space = generate_krc(
            &[make_line(500, 2000, syllables_without_space)],
            &MetadataStore::new(),
        )
        .unwrap();
        let output_with_space = generate_krc(
            &[make_line(500, 2000, syllables_with_space)],
            &MetadataStore::new(),
        )
        .unwrap();

        assert_eq!(
            syllable_timestamps(&output_with_space),
            vec!["0,300,0", "400,400,0", "800,200,0"]
        );
        assert_eq!(
            syllable_timestamps(&output_with_space),
            syllable_timestamps(&output_without_space),
            "空格标记不应影响音节时间标签"
        );
        assert_eq!(
            lyric_line(&output_with_space),
            "[500,1500]<0,300,0>探 <400,400,0>测 <800,200,0>行"
        );
    }

    // 端到端回归：TTML 中相邻 `<span>` 之间的空格，经解析后必须保留到生成的 KRC 中。
    #[test]
    fn test_ttml_span_separator_space_survives_krc_generation() {
        let ttml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xmlns:itunes="http://music.apple.com/lyric-ttml-internal" itunes:timing="word">
  <head/>
  <body>
    <div>
      <p begin="5.000s" end="8.000s">
        <span begin="5.000s" end="5.500s">Hello</span> <span begin="5.600s" end="6.200s">world</span>
      </p>
    </div>
  </body>
</tt>"#;

        let parsed =
            ttml_processor::parse_ttml(ttml, &lyrics_helper_core::TtmlParsingOptions::default())
                .expect("解析 TTML 失败");

        let output = generate_krc(&parsed.lines, &MetadataStore::new()).unwrap();

        assert_eq!(
            lyric_line(&output),
            "[5000,3000]<0,500,0>Hello <600,600,0>world"
        );
    }

    // 测试 E：行末空格行为。
    //
    // `ends_with_space` 同时用于表示 TTML 中两个 `<span>` 之间的空格和行尾空白。
    // 按照字段语义（“必须根据此标志在音节后附加空格”），最后一个音节标记为
    // `true` 时也应输出空格；KRC 解析器会把行尾空格解析回该标志，往返一致。
    // 因此这里保留行尾空格，不加入额外的 `trim()` 裁剪逻辑。
    #[test]
    fn test_line_final_space_is_kept_according_to_field_semantics() {
        let lines = vec![make_line(
            0,
            200,
            vec![syl("探", 0, 100, false), syl("测", 100, 200, true)],
        )];

        let output = generate_krc(&lines, &MetadataStore::new()).unwrap();

        assert_eq!(lyric_line(&output), "[0,200]<0,100,0>探<100,100,0>测 ");
    }
}
