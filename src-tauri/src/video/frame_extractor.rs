use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer::{self as gst, ElementFactory};
use gstreamer_app::AppSink;
use image::{ImageBuffer, Rgb};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// 指定された VideoInfo と元データ（stride を含む可能性あり）から
// 連続した RGB バイト列を作成して返す。
fn plane_to_contiguous_rgb(video_info: &gstreamer_video::VideoInfo, src: &[u8]) -> Vec<u8> {
    let width = video_info.width() as usize;
    let height = video_info.height() as usize;
    // stride() は行バイト幅のスライスを返す（通常は1要素）
    let stride = video_info.stride().get(0).cloned().unwrap_or((width * 3) as i32) as usize;

    // stride が期待どおりならそのままコピー
    if stride == width * 3 {
        return src.to_vec();
    }

    let mut out = Vec::with_capacity(width * 3 * height);
    for row in 0..height {
        let start = row * stride;
        let end = start + width * 3;
        if end <= src.len() {
            out.extend_from_slice(&src[start..end]);
        } else if start < src.len() {
            // 不足している場合は残りをコピーしてゼロ埋め
            out.extend_from_slice(&src[start..src.len()]);
            out.extend(std::iter::repeat(0).take(end - src.len()));
        } else {
            out.extend(std::iter::repeat(0).take(width * 3));
        }
    }

    out
}

/// フレーム抽出の設定
#[derive(Debug, Clone)]
pub struct FrameExtractorConfig {
    /// フレーム抽出間隔（フレーム数）。1なら全フレーム、30なら30フレームごと
    pub frame_interval: u32,
    /// 出力ディレクトリ
    pub output_dir: PathBuf,
    /// 出力画像のフォーマット（例: "png", "jpg"）
    pub image_format: String,
    /// JPEGの品質（0-100、jpgの場合のみ有効）
    pub jpeg_quality: u8,
}

impl Default for FrameExtractorConfig {
    fn default() -> Self {
        Self {
            frame_interval: 1,
            output_dir: PathBuf::from("output/frames"),
            image_format: "png".to_string(),
            jpeg_quality: 95,
        }
    }
}

/// 動画情報
#[derive(Debug, Clone)]
pub struct CustomVideoInfo {
    pub width: i32,
    pub height: i32,
    pub fps: f64,
    pub duration_sec: f64,
}

/// フレーム抽出器
pub struct FrameExtractor {
    config: FrameExtractorConfig,
}

impl FrameExtractor {
    /// 新しいフレーム抽出器を作成
    pub fn new(config: FrameExtractorConfig) -> Self {
        Self { config }
    }

    /// デフォルト設定でフレーム抽出器を作成
    pub fn default() -> Self {
        Self {
            config: FrameExtractorConfig::default(),
        }
    }

    /// GStreamerを初期化
    fn init_gstreamer() -> Result<()> {
        gst::init().context("GStreamerの初期化に失敗しました")?;
        Ok(())
    }

    /// 動画ファイルの情報を取得
    pub fn get_video_info<P: AsRef<Path>>(video_path: P) -> Result<CustomVideoInfo> {
        Self::init_gstreamer()?;

        let video_path = video_path.as_ref();
        
        // ファイルの存在チェック
        if !video_path.exists() {
            anyhow::bail!("動画ファイルが見つかりません: {:?}", video_path);
        }
        
        // ファイルが読み取り可能かチェック
        if let Err(e) = std::fs::metadata(video_path) {
            anyhow::bail!("動画ファイルにアクセスできません: {:?} ({})", video_path, e);
        }
        
        let canonical = video_path
            .canonicalize()
            .context("動画ファイルのパスを解決できませんでした")?;
        let uri = url::Url::from_file_path(&canonical)
            .map_err(|_| anyhow::anyhow!("ファイルパスからURIへの変換に失敗しました"))?
            .to_string();

        // Discovererを使って動画情報を取得
        let discoverer = gstreamer_pbutils::Discoverer::new(gst::ClockTime::from_seconds(10))
            .context("Discovererの作成に失敗しました")?;

        let info = discoverer
            .discover_uri(&uri)
            .context("動画の解析に失敗しました")?;

        let video_streams = info.video_streams();
        if video_streams.is_empty() {
            anyhow::bail!("動画ストリームが見つかりません");
        }

        let video_stream = &video_streams[0];
        let width = video_stream.width() as i32;
        let height = video_stream.height() as i32;
        let fps_num = video_stream.framerate().numer() as f64;
        let fps_den = video_stream.framerate().denom() as f64;
        let fps = fps_num / fps_den;

        let duration = info.duration();
        let duration_sec = if let Some(dur) = duration {
            dur.seconds() as f64
        } else {
            0.0
        };

        Ok(CustomVideoInfo {
            width,
            height,
            fps,
            duration_sec,
        })
    }

    /// 動画からフレームを抽出（進捗コールバック付き）
    pub fn extract_frames_with_progress<P, F>(
        &self,
        video_path: P,
        progress_callback: Option<F>,
        crop_region: Option<crate::analyzer::InputIndicatorRegion>,
    ) -> Result<Vec<PathBuf>>
    where
        P: AsRef<Path>,
        F: Fn(usize) + Send + Sync + 'static,
    {
        Self::init_gstreamer()?;

        let video_path = video_path.as_ref();
        
        // ファイルの存在チェック
        if !video_path.exists() {
            anyhow::bail!("動画ファイルが見つかりません: {:?}", video_path);
        }
        
        // ファイルが読み取り可能かチェック
        if let Err(e) = std::fs::metadata(video_path) {
            anyhow::bail!("動画ファイルにアクセスできません: {:?} ({})", video_path, e);
        }
        
        println!("動画ファイルを開いています: {}", video_path.display());

        // 出力ディレクトリを作成
        std::fs::create_dir_all(&self.config.output_dir)
            .context("出力ディレクトリの作成に失敗しました")?;

        // 動画情報を取得
        let info = Self::get_video_info(video_path)?;
        println!("動画情報:");
        println!("  解像度: {}x{}", info.width, info.height);
        println!("  FPS: {:.2}", info.fps);
        println!("  再生時間: {:.2}秒", info.duration_sec);

        let _canonical = video_path.canonicalize()?;
        let _uri = url::Url::from_file_path(&_canonical)
            .map_err(|_| anyhow::anyhow!("ファイルパスからURIへの変換に失敗しました"))?
            .to_string();

        // GStreamerパイプラインを構築
        let pipeline = gst::Pipeline::new();

        // エレメントを作成
        let source = ElementFactory::make("filesrc")
            .name("source")
            .build()
            .context("filesrcの作成に失敗しました")?;

        let decodebin = ElementFactory::make("decodebin")
            .name("decoder")
            .build()
            .context("decodebinの作成に失敗しました")?;

        let videoconvert = ElementFactory::make("videoconvert")
            .name("converter")
            .build()
            .context("videoconvertの作成に失敗しました")?;

        let appsink = ElementFactory::make("appsink")
            .name("sink")
            .build()
            .context("appsinkの作成に失敗しました")?;

        let appsink = appsink
            .dynamic_cast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("appsinkへのキャストに失敗しました"))?;

        // AppSinkの設定
        appsink.set_caps(Some(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGB")
                .build(),
        ));
        appsink.set_property("emit-signals", false);
        appsink.set_property("sync", false);

        // ファイルパスを設定（正規化した絶対パスを使用）
        let source_path = video_path.canonicalize()?;
        source.set_property("location", source_path.to_str().unwrap());

        // パイプラインにエレメントを追加
        // source と decodebin の追加は共通
        // videocrop を使う場合は videocrop をパイプラインに挿入して
        // videoconvert -> videocrop -> appsink の形にする
        if let Some(region) = &crop_region {
            let videocrop = ElementFactory::make("videocrop")
                .name("crop")
                .build()
                .context("videocrop の作成に失敗しました")?;

            // crop の値を計算
            let video_w = info.width as i32;
            let video_h = info.height as i32;
            let left = region.x as i32;
            let top = region.y as i32;
            let right = (video_w - (region.x as i32 + region.width as i32)).max(0);
            let bottom = (video_h - (region.y as i32 + region.height as i32)).max(0);

            videocrop.set_property("left", left);
            videocrop.set_property("top", top);
            videocrop.set_property("right", right);
            videocrop.set_property("bottom", bottom);

            pipeline.add_many(&[
                &source,
                &decodebin,
                &videoconvert,
                videocrop.upcast_ref::<gst::Element>(),
                appsink.upcast_ref::<gst::Element>(),
            ])
            .context("エレメントの追加に失敗しました")?;

            // source と decodebin をリンク
            source
                .link(&decodebin)
                .context("sourceとdecoderのリンクに失敗しました")?;

            // videoconvert -> videocrop -> appsink をリンク
            videoconvert
                .link(videocrop.upcast_ref::<gst::Element>())
                .context("converterとvideocropのリンクに失敗しました")?;
            videocrop
                .link(appsink.upcast_ref::<gst::Element>())
                .context("videocropとsinkのリンクに失敗しました")?;
        } else {
            pipeline
                .add_many(&[
                    &source,
                    &decodebin,
                    &videoconvert,
                    appsink.upcast_ref::<gst::Element>(),
                ])
                .context("エレメントの追加に失敗しました")?;

            // sourceとdecodebinをリンク
            source
                .link(&decodebin)
                .context("sourceとdecoderのリンクに失敗しました")?;

            // videoconvertとappsinkをリンク
            videoconvert
                .link(appsink.upcast_ref::<gst::Element>())
                .context("converterとsinkのリンクに失敗しました")?;
        }

        // decodebinの動的パッドをリンク
        let videoconvert_clone = videoconvert.clone();
        decodebin.connect_pad_added(move |_src, src_pad| {
            let sink_pad = videoconvert_clone
                .static_pad("sink")
                .expect("videoconvertのsinkパッドが見つかりません");

            if !sink_pad.is_linked() {
                if let Err(e) = src_pad.link(&sink_pad) {
                    eprintln!("パッドのリンクに失敗: {:?}", e);
                }
            }
        });

        println!("\nフレーム抽出中...");
        println!("  抽出間隔: {}フレームごと", self.config.frame_interval);
        println!("  出力先: {}", self.config.output_dir.display());

        let output_paths = Arc::new(Mutex::new(Vec::new()));
        let frame_count = Arc::new(Mutex::new(0u32));
        let extracted_count = Arc::new(Mutex::new(0u32));

        // 必要なフレーム数に達したら停止するためのフラグ
        // frame_intervalが非常に大きい場合（frame 0のみ）は、1フレーム抽出後に停止
        let should_stop = Arc::new(Mutex::new(false));
        let target_extracts = if self.config.frame_interval == u32::MAX { 1 } else { u32::MAX };

        let progress_callback = Arc::new(progress_callback);
        let output_paths_clone = output_paths.clone();
        let frame_count_clone = frame_count.clone();
        let extracted_count_clone = extracted_count.clone();
        let should_stop_clone = should_stop.clone();
        let progress_callback_clone = progress_callback.clone();
        let config = self.config.clone();

        // サンプルコールバックを設定
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;

                    let video_info = gstreamer_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;

                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;

                    let mut frame_num = frame_count_clone.lock().unwrap();
                    let current_frame = *frame_num;
                    *frame_num += 1;

                    // 指定された間隔でフレームを保存
                    if current_frame % config.frame_interval == 0 {
                        let width = video_info.width() as u32;
                        let height = video_info.height() as u32;

                        // RGB画像として保存（stride に対応して連続バッファを作成）
                        let contiguous = plane_to_contiguous_rgb(&video_info, map.as_slice());
                        if let Some(img_buffer) =
                            ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, contiguous)
                        {
                            let filename = format!("frame_{:06}.{}", current_frame, config.image_format);
                            let output_path = config.output_dir.join(&filename);

                            if let Err(e) = if config.image_format == "jpg" || config.image_format == "jpeg" {
                                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                                    std::fs::File::create(&output_path).unwrap(),
                                    config.jpeg_quality,
                                );
                                img_buffer.write_with_encoder(encoder)
                            } else {
                                img_buffer.save(&output_path)
                            } {
                                eprintln!("フレームの保存に失敗: {}", e);
                            } else {
                                let mut paths = output_paths_clone.lock().unwrap();
                                paths.push(output_path);

                                let mut extracted = extracted_count_clone.lock().unwrap();
                                *extracted += 1;

                                // 進捗コールバック呼び出し
                                if let Some(ref callback) = *progress_callback_clone {
                                    callback(*extracted as usize);
                                }

                                if *extracted % 10 == 0 {
                                    println!("  {}フレーム抽出完了", *extracted);
                                }

                                // 必要なフレーム数に達したら停止フラグを立てる
                                if *extracted >= target_extracts {
                                    let mut stop = should_stop_clone.lock().unwrap();
                                    *stop = true;
                                }
                            }
                        }
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        // パイプラインを開始
        pipeline
            .set_state(gst::State::Playing)
            .context("パイプラインの開始に失敗しました")?;

        // バスメッセージを処理
        let bus = pipeline
            .bus()
            .expect("パイプラインにバスがありません");

        for msg in bus.iter_timed(gst::ClockTime::NONE) {
            use gst::MessageView;

            match msg.view() {
                MessageView::Eos(..) => {
                    println!("\n動画の終わりに到達しました");
                    break;
                }
                MessageView::Error(err) => {
                    pipeline.set_state(gst::State::Null).ok();
                    anyhow::bail!(
                        "エラーが発生しました: {} (デバッグ情報: {:?})",
                        err.error(),
                        err.debug()
                    );
                }
                _ => (),
            }

            // 必要なフレーム数に達したら停止
            if *should_stop.lock().unwrap() {
                println!("\n必要なフレーム数に達しました。処理を停止します。");
                break;
            }
        }

        // パイプラインを停止
        pipeline
            .set_state(gst::State::Null)
            .context("パイプラインの停止に失敗しました")?;

        let final_frame_count = *frame_count.lock().unwrap();
        let final_extracted_count = *extracted_count.lock().unwrap();

        println!("\n抽出完了!");
        println!("  処理フレーム数: {}", final_frame_count);
        println!("  抽出フレーム数: {}", final_extracted_count);

        let paths = Arc::try_unwrap(output_paths)
            .map(|m| m.into_inner().unwrap())
            .unwrap_or_else(|arc| arc.lock().unwrap().clone());

        Ok(paths)
    }

    /// 動画からフレームを抽出
    pub fn extract_frames<P: AsRef<Path>>(&self, video_path: P) -> Result<Vec<PathBuf>> {
        self.extract_frames_with_progress(video_path, None::<fn(usize)>, None)
    }

    /// 動画からフレームを1つずつコールバックで処理
    ///
    /// # Arguments
    /// * `video_path` - 動画ファイルパス
    /// * `callback` - 各フレームのパスを受け取るコールバック関数。Err を返すと処理を中断
    pub fn extract_frames_with_callback<P, F>(
        &self,
        video_path: P,
        callback: F,
    ) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(PathBuf) -> Result<()> + Send + 'static,
    {
        Self::init_gstreamer()?;

        let video_path = video_path.as_ref();
        
        // ファイルの存在チェック
        if !video_path.exists() {
            anyhow::bail!("動画ファイルが見つかりません: {:?}", video_path);
        }
        
        // ファイルが読み取り可能かチェック
        if let Err(e) = std::fs::metadata(video_path) {
            anyhow::bail!("動画ファイルにアクセスできません: {:?} ({})", video_path, e);
        }
        
        println!("動画ファイルを開いています: {}", video_path.display());

        // 出力ディレクトリを作成
        std::fs::create_dir_all(&self.config.output_dir)
            .context("出力ディレクトリの作成に失敗しました")?;

        // 動画情報を取得
        let info = Self::get_video_info(video_path)?;
        println!("動画情報:");
        println!("  解像度: {}x{}", info.width, info.height);
        println!("  FPS: {:.2}", info.fps);
        println!("  再生時間: {:.2}秒", info.duration_sec);

        // GStreamerパイプラインを構築
        let pipeline = gst::Pipeline::new();

        let source = ElementFactory::make("filesrc")
            .name("source")
            .build()
            .context("filesrcの作成に失敗しました")?;

        let decodebin = ElementFactory::make("decodebin")
            .name("decoder")
            .build()
            .context("decodebinの作成に失敗しました")?;

        let videoconvert = ElementFactory::make("videoconvert")
            .name("converter")
            .build()
            .context("videoconvertの作成に失敗しました")?;

        let appsink = ElementFactory::make("appsink")
            .name("sink")
            .build()
            .context("appsinkの作成に失敗しました")?;

        let appsink = appsink
            .dynamic_cast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("appsinkへのキャストに失敗しました"))?;

        appsink.set_caps(Some(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGB")
                .build(),
        ));
        appsink.set_property("emit-signals", false);
        appsink.set_property("sync", false);

        let source_path = video_path.canonicalize()?;
        source.set_property("location", source_path.to_str().unwrap());

        pipeline
            .add_many(&[&source, &decodebin, &videoconvert, appsink.upcast_ref::<gst::Element>()])
            .context("エレメントの追加に失敗しました")?;

        source.link(&decodebin).context("sourceとdecoderのリンクに失敗しました")?;
        videoconvert.link(appsink.upcast_ref::<gst::Element>())
            .context("converterとsinkのリンクに失敗しました")?;

        let videoconvert_clone = videoconvert.clone();
        decodebin.connect_pad_added(move |_src, src_pad| {
            let sink_pad = videoconvert_clone
                .static_pad("sink")
                .expect("videoconvertのsinkパッドが見つかりません");

            if !sink_pad.is_linked() {
                if let Err(e) = src_pad.link(&sink_pad) {
                    eprintln!("パッドのリンクに失敗: {:?}", e);
                }
            }
        });

        let frame_count = Arc::new(Mutex::new(0u32));
        let extracted_count = Arc::new(Mutex::new(0u32));
        let callback_error = Arc::new(Mutex::new(None::<String>));
        let callback = Arc::new(Mutex::new(callback));

        let frame_count_clone = frame_count.clone();
        let extracted_count_clone = extracted_count.clone();
        let callback_error_clone = callback_error.clone();
        let callback_clone = callback.clone();
        let config = self.config.clone();

        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    // エラーが既に発生していたら処理を中断
                    if callback_error_clone.lock().unwrap().is_some() {
                        return Err(gst::FlowError::Error);
                }

                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Error)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let caps = sample.caps().ok_or(gst::FlowError::Error)?;

                    let video_info = gstreamer_video::VideoInfo::from_caps(caps)
                        .map_err(|_| gst::FlowError::Error)?;

                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;

                    let mut frame_num = frame_count_clone.lock().unwrap();
                    let current_frame = *frame_num;
                    *frame_num += 1;

                    if current_frame % config.frame_interval == 0 {
                        let width = video_info.width() as u32;
                        let height = video_info.height() as u32;

                        let contiguous = plane_to_contiguous_rgb(&video_info, map.as_slice());
                        let img = image::RgbImage::from_raw(width, height, contiguous)
                            .ok_or(gst::FlowError::Error)?;

                        let output_filename = format!("frame_{:08}.{}", current_frame, config.image_format);
                        let output_path = config.output_dir.join(&output_filename);

                        if let Err(e) = img.save(&output_path) {
                            eprintln!("画像保存エラー: {}", e);
                            return Err(gst::FlowError::Error);
                        }

                        let mut extracted = extracted_count_clone.lock().unwrap();
                        *extracted += 1;

                        // コールバックを呼び出し
                        let result = {
                            let mut cb = callback_clone.lock().unwrap();
                            cb(output_path)
                        };

                        if let Err(e) = result {
                            *callback_error_clone.lock().unwrap() = Some(format!("コールバックエラー: {}", e));
                            return Err(gst::FlowError::Error);
                        }
                    }

                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        pipeline.set_state(gst::State::Playing)
            .context("パイプラインの開始に失敗しました")?;

        let bus = pipeline.bus().expect("パイプラインにバスがありません");

        for msg in bus.iter_timed(gst::ClockTime::NONE) {
            use gst::MessageView;

            match msg.view() {
                MessageView::Eos(..) => {
                    break;
                }
                MessageView::Error(err) => {
                    pipeline.set_state(gst::State::Null).ok();
                    anyhow::bail!(
                        "エラーが発生しました: {} (デバッグ情報: {:?})",
                        err.error(),
                        err.debug()
                    );
                }
                _ => (),
            }
        }

        pipeline.set_state(gst::State::Null)
            .context("パイプラインの停止に失敗しました")?;

        // コールバックでエラーが発生していたら返す
        if let Some(error) = callback_error.lock().unwrap().take() {
            anyhow::bail!(error);
        }

        let final_frame_count = *frame_count.lock().unwrap();
        let final_extracted_count = *extracted_count.lock().unwrap();

        println!("\n抽出完了!");
        println!("  処理フレーム数: {}", final_frame_count);
        println!("  抽出フレーム数: {}", final_extracted_count);

        Ok(())
    }

    /// シーク後、指定フレーム位置の単一フレームをデコード
    pub fn extract_frame_at_seek<P: AsRef<Path>>(
        &self,
        video_path: P,
        frame_number: u32,
    ) -> Result<PathBuf> {
        Self::init_gstreamer()?;

        let video_path = video_path.as_ref();
        let info = Self::get_video_info(video_path)?;

        // フレーム番号から時間（秒）を計算
        let time_sec = (frame_number as f64) / info.fps;
        let time_ns = gst::ClockTime::from_seconds(time_sec as u64);

        // 出力ディレクトリを作成
        std::fs::create_dir_all(&self.config.output_dir)
            .context("出力ディレクトリの作成に失敗しました")?;

        // GStreamerパイプラインを構築
        let pipeline = gst::Pipeline::new();

        let canonical = video_path.canonicalize()?;
        let source = ElementFactory::make("filesrc")
            .property("location", canonical.to_str().unwrap())
            .build()
            .context("filesrcの作成に失敗しました")?;

        let decodebin = ElementFactory::make("decodebin")
            .build()
            .context("decodebinの作成に失敗しました")?;

        let videoconvert = ElementFactory::make("videoconvert")
            .build()
            .context("videoconvertの作成に失敗しました")?;

        let appsink = ElementFactory::make("appsink")
            .build()
            .context("appsinkの作成に失敗しました")?;

        let appsink = appsink
            .dynamic_cast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("appsinkへのキャストに失敗しました"))?;

        appsink.set_caps(Some(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGB")
                .build(),
        ));
        appsink.set_property("emit-signals", false);
        appsink.set_property("sync", false);

        pipeline
            .add_many(&[&source, &decodebin, &videoconvert, appsink.upcast_ref::<gst::Element>()])
            .context("エレメントの追加に失敗しました")?;

        source
            .link(&decodebin)
            .context("sourceとdecoderのリンクに失敗しました")?;

        videoconvert
            .link(appsink.upcast_ref::<gst::Element>())
            .context("converterとsinkのリンクに失敗しました")?;

        // decodebinの動的パッドをリンク
        let videoconvert_clone = videoconvert.clone();
        decodebin.connect_pad_added(move |_dbin, pad| {
            if pad.name().starts_with("video") {
                let videoconvert_sink = videoconvert_clone.static_pad("sink").unwrap();
                let _ = pad.link(&videoconvert_sink);
            }
        });

        // パイプラインを再生状態に
        pipeline
            .set_state(gst::State::Playing)
            .context("パイプラインの開始に失敗しました")?;

        // シーク処理
        pipeline.seek_simple(gst::SeekFlags::FLUSH, time_ns)?;

        // AppSinkからサンプルを取得
        let _appsink_element = appsink.upcast_ref::<gst::Element>();

        // パイプラインを停止するまでサンプルを待機
        std::thread::sleep(std::time::Duration::from_millis(100));

        // AppSinkからサンプルを取得
        let output_paths = Arc::new(Mutex::new(Vec::new()));
        let output_paths_clone = output_paths.clone();

        if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::NONE) {
            if let Some(buffer) = sample.buffer() {
                if let Ok(map) = buffer.map_readable() {
                    let caps = sample.caps().unwrap();
                    if let Some(structure) = caps.structure(0) {
                        if let (Ok(width), Ok(height)) = (
                            structure.get::<i32>("width"),
                            structure.get::<i32>("height"),
                        ) {
                            // 画像を保存
                            let frame_data = map.as_slice();
                            // caps から VideoInfo を作成して stride を考慮してコピー
                            if let Ok(video_info2) = gstreamer_video::VideoInfo::from_caps(&caps) {
                                let contiguous = plane_to_contiguous_rgb(&video_info2, frame_data);
                                if let Some(img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
                                    width as u32,
                                    height as u32,
                                    contiguous,
                                ) {
                                    let output_path = self.config.output_dir.join(format!("frame_{:06}.png", frame_number));
                                    if let Ok(_) = img.save(&output_path) {
                                        output_paths_clone.lock().unwrap().push(output_path);
                                    }
                                }
                            } else {
                                // VideoInfo 作成失敗時は従来どおり直接保存
                                if let Some(img) = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
                                    width as u32,
                                    height as u32,
                                    frame_data.to_vec(),
                                ) {
                                    let output_path = self.config.output_dir.join(format!("frame_{:06}.png", frame_number));
                                    if let Ok(_) = img.save(&output_path) {
                                        output_paths_clone.lock().unwrap().push(output_path);
                                    }
                                }
                            }
                            
                        }
                    }
                }
            }
        }

        pipeline
            .set_state(gst::State::Null)
            .context("パイプラインの停止に失敗しました")?;

        let paths = output_paths.lock().unwrap().clone();
        paths
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("フレームの抽出に失敗しました"))
    }

    /// 特定のフレーム番号のフレームを抽出
    pub fn extract_frame_at<P: AsRef<Path>>(
        &self,
        video_path: P,
        frame_number: u32,
    ) -> Result<PathBuf> {
        // frame 0の場合は最初のフレームだけを抽出
        if frame_number == 0 {
            // 最初のフレームのみ抽出するため、frame_intervalを非常に大きく設定
            let mut temp_config = self.config.clone();
            // frame_intervalを最初のフレームより大きく設定することで、
            // 最初のフレーム（frame 0）のみが抽出される
            temp_config.frame_interval = u32::MAX; // 最初のフレームのみを抽出

            let temp_extractor = FrameExtractor::new(temp_config);
            let paths = temp_extractor.extract_frames(&video_path)?;

            // 最初に抽出されたフレームを返す
            paths
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("フレームの抽出に失敗しました"))
        } else {
            // その他のフレームは従来の方法で抽出
            let mut temp_config = self.config.clone();
            temp_config.frame_interval = (frame_number + 1).max(1);

            let temp_extractor = FrameExtractor::new(temp_config);
            let paths = temp_extractor.extract_frames(&video_path)?;

            // 最後に抽出されたフレームが目的のフレーム
            paths
                .into_iter()
                .last()
                .ok_or_else(|| anyhow::anyhow!("フレームの抽出に失敗しました"))
        }
    }

    /// 時間指定でフレームを抽出（秒単位）
    pub fn extract_frame_at_time<P: AsRef<Path>>(
        &self,
        video_path: P,
        time_sec: f64,
    ) -> Result<PathBuf> {
        let info = Self::get_video_info(&video_path)?;
        let frame_number = (time_sec * info.fps) as u32;
        self.extract_frame_at(video_path, frame_number)
    }

    /// 動画からフレームを抽出し、各フレームをメモリ上で同期的にコールバックで処理
    /// 
    /// GStreamerのSend制約を回避するため、AppSinkから取得したバッファを
    /// 同じスレッド内でコールバックに渡す。これによりWgpuなどのnon-Send型も使用可能。
    ///
    /// # Arguments
    /// * `video_path` - 動画ファイルパス
    /// * `callback` - 各フレームの画像データを受け取るコールバック関数
    pub fn process_frames_sync<P, F>(
        &self,
        video_path: P,
        mut callback: F,
    ) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(&image::RgbImage, u32) -> Result<()>,
    {
        Self::init_gstreamer()?;

        let video_path = video_path.as_ref();
        println!("動画ファイルを開いています: {}", video_path.display());

        // 動画情報を取得
        let info = Self::get_video_info(video_path)?;
        println!("動画情報:");
        println!("  解像度: {}x{}", info.width, info.height);
        println!("  FPS: {:.2}", info.fps);
        println!("  再生時間: {:.2}秒", info.duration_sec);

        // GStreamerパイプラインを構築
        let pipeline = gst::Pipeline::new();

        let source = ElementFactory::make("filesrc")
            .name("source")
            .build()
            .context("filesrcの作成に失敗しました")?;

        let decodebin = ElementFactory::make("decodebin")
            .name("decoder")
            .build()
            .context("decodebinの作成に失敗しました")?;

        let videoconvert = ElementFactory::make("videoconvert")
            .name("converter")
            .build()
            .context("videoconvertの作成に失敗しました")?;

        let appsink = ElementFactory::make("appsink")
            .name("sink")
            .build()
            .context("appsinkの作成に失敗しました")?;

        let appsink = appsink
            .dynamic_cast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("appsinkへのキャストに失敗しました"))?;

        appsink.set_caps(Some(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGB")
                .build(),
        ));
        appsink.set_property("emit-signals", false);
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);  // バッファを最小化

        source.set_property("location", video_path.to_str().unwrap());

        pipeline
            .add_many(&[&source, &decodebin, &videoconvert, appsink.upcast_ref::<gst::Element>()])
            .context("エレメントの追加に失敗しました")?;

        source.link(&decodebin).context("sourceとdecoderのリンクに失敗しました")?;
        videoconvert.link(appsink.upcast_ref::<gst::Element>())
            .context("converterとsinkのリンクに失敗しました")?;

        let videoconvert_clone = videoconvert.clone();
        decodebin.connect_pad_added(move |_src, src_pad| {
            let sink_pad = videoconvert_clone
                .static_pad("sink")
                .expect("videoconvertのsinkパッドが見つかりません");

            if !sink_pad.is_linked() {
                if let Err(e) = src_pad.link(&sink_pad) {
                    eprintln!("パッドのリンクに失敗: {:?}", e);
                }
            }
        });

        pipeline.set_state(gst::State::Playing)
            .context("パイプラインの開始に失敗しました")?;

        let bus = pipeline.bus().expect("パイプラインにバスがありません");
        let mut frame_count = 0u32;
        let mut processed_count = 0u32;

        // フレームを同期的に処理
        loop {
            // バスメッセージを確認
            if let Some(msg) = bus.pop() {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(..) => {
                        break;
                    }
                    MessageView::Error(err) => {
                        pipeline.set_state(gst::State::Null).ok();
                        anyhow::bail!(
                            "エラーが発生しました: {} (デバッグ情報: {:?})",
                            err.error(),
                            err.debug()
                        );
                    }
                    _ => (),
                }
            }

            // フレームを取得（非ブロッキング）
            if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
                let buffer = sample.buffer().context("バッファの取得に失敗しました")?;
                let caps = sample.caps().context("capsの取得に失敗しました")?;

                let video_info = gstreamer_video::VideoInfo::from_caps(caps)
                    .context("VideoInfoの作成に失敗しました")?;

                let map = buffer.map_readable().context("バッファのマップに失敗しました")?;

                let current_frame = frame_count;
                frame_count += 1;

                if current_frame % self.config.frame_interval == 0 {
                    let width = video_info.width() as u32;
                    let height = video_info.height() as u32;

                    let contiguous = plane_to_contiguous_rgb(&video_info, map.as_slice());
                    let img = image::RgbImage::from_raw(width, height, contiguous)
                        .context("RgbImageの作成に失敗しました")?;

                    // コールバックを同期的に呼び出し（同じスレッド内）
                    callback(&img, current_frame)?;

                    processed_count += 1;

                    if processed_count % 30 == 0 {
                        println!("処理済み: {}フレーム", processed_count);
                    }
                }
            }
        }

        pipeline.set_state(gst::State::Null)
            .context("パイプラインの停止に失敗しました")?;

        println!("\n処理完了!");
        println!("  総フレーム数: {}", frame_count);
        println!("  処理フレーム数: {}", processed_count);

        Ok(())
    }

    /// 動画をクロップしてからフレームを同期的に処理する
    ///
    /// `crop_region` が Some の場合、GStreamer パイプラインに `videocrop` を挿入し、
    /// 指定領域を先に切り出してから AppSink に渡します。AppSink に渡される画像は
    /// 切り出し後の領域（幅 = crop_region.width, 高さ = crop_region.height）になります。
    pub fn process_frames_sync_with_crop<P, F>(
        &self,
        video_path: P,
        crop_region: Option<crate::analyzer::InputIndicatorRegion>,
        mut callback: F,
    ) -> Result<()>
    where
        P: AsRef<Path>,
        F: FnMut(&image::RgbImage, u32) -> Result<()>,
    {
        Self::init_gstreamer()?;

        let video_path = video_path.as_ref();
        println!("動画ファイルを開いています: {}", video_path.display());

        // 動画情報を取得
        let info = Self::get_video_info(video_path)?;
        println!("動画情報:");
        println!("  解像度: {}x{}", info.width, info.height);
        println!("  FPS: {:.2}", info.fps);
        println!("  再生時間: {:.2}秒", info.duration_sec);

        // GStreamerパイプラインを構築
        let pipeline = gst::Pipeline::new();

        let source = ElementFactory::make("filesrc")
            .name("source")
            .build()
            .context("filesrcの作成に失敗しました")?;

        let decodebin = ElementFactory::make("decodebin")
            .name("decoder")
            .build()
            .context("decodebinの作成に失敗しました")?;

        let videoconvert = ElementFactory::make("videoconvert")
            .name("converter")
            .build()
            .context("videoconvertの作成に失敗しました")?;

        // videocrop はオプションで追加
        let videocrop = if crop_region.is_some() {
            Some(
                ElementFactory::make("videocrop")
                    .name("crop")
                    .build()
                    .context("videocropの作成に失敗しました")?,
            )
        } else {
            None
        };

        let appsink = ElementFactory::make("appsink")
            .name("sink")
            .build()
            .context("appsinkの作成に失敗しました")?;

        let appsink = appsink
            .dynamic_cast::<AppSink>()
            .map_err(|_| anyhow::anyhow!("appsinkへのキャストに失敗しました"))?;

        appsink.set_caps(Some(
            &gst::Caps::builder("video/x-raw").field("format", "RGB").build(),
        ));
        appsink.set_property("emit-signals", false);
        appsink.set_property("sync", false);
        appsink.set_property("max-buffers", 1u32);

        source.set_property("location", video_path.to_str().unwrap());

        // パイプラインにエレメントを追加
        if let Some(ref crop) = videocrop {
            pipeline
                .add_many(&[&source, &decodebin, &videoconvert, crop, appsink.upcast_ref::<gst::Element>()])
                .context("エレメントの追加に失敗しました")?;
        } else {
            pipeline
                .add_many(&[&source, &decodebin, &videoconvert, appsink.upcast_ref::<gst::Element>()])
                .context("エレメントの追加に失敗しました")?;
        }

        source.link(&decodebin).context("sourceとdecoderのリンクに失敗しました")?;

        // パス: decodebin -> videoconvert -> (videocrop?) -> appsink
        if let Some(ref crop) = videocrop {
            videoconvert
                .link(crop)
                .context("converterとvideocropのリンクに失敗しました")?;
            crop.link(appsink.upcast_ref::<gst::Element>())
                .context("videocropとsinkのリンクに失敗しました")?;
        } else {
            videoconvert
                .link(appsink.upcast_ref::<gst::Element>())
                .context("converterとsinkのリンクに失敗しました")?;
        }

        let videoconvert_clone = videoconvert.clone();
        decodebin.connect_pad_added(move |_src, src_pad| {
            let sink_pad = videoconvert_clone
                .static_pad("sink")
                .expect("videoconvertのsinkパッドが見つかりません");

            if !sink_pad.is_linked() {
                if let Err(e) = src_pad.link(&sink_pad) {
                    eprintln!("パッドのリンクに失敗: {:?}", e);
                }
            }
        });

        // videocrop プロパティ設定（必要なら）
        if let (Some(crop_elem), Some(region)) = (videocrop.as_ref(), crop_region) {
            let left = region.x as i32;
            let top = region.y as i32;
            let crop_w = region.width as i32;
            let crop_h = region.height as i32;
            let right = (info.width as i32) - (left + crop_w);
            let bottom = (info.height as i32) - (top + crop_h);
            let right = if right < 0 { 0 } else { right };
            let bottom = if bottom < 0 { 0 } else { bottom };

            crop_elem.set_property("left", &left);
            crop_elem.set_property("right", &right);
            crop_elem.set_property("top", &top);
            crop_elem.set_property("bottom", &bottom);
        }

        pipeline.set_state(gst::State::Playing)
            .context("パイプラインの開始に失敗しました")?;

        let bus = pipeline.bus().expect("パイプラインにバスがありません");
        let mut frame_count = 0u32;
        let mut processed_count = 0u32;

        // フレームを同期的に処理
        loop {
            // バスメッセージを確認
            if let Some(msg) = bus.pop() {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(..) => {
                        break;
                    }
                    MessageView::Error(err) => {
                        pipeline.set_state(gst::State::Null).ok();
                        anyhow::bail!(
                            "エラーが発生しました: {} (デバッグ情報: {:?})",
                            err.error(),
                            err.debug()
                        );
                    }
                    _ => (),
                }
            }

            // フレームを取得（非ブロッキング）
            if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
                let buffer = sample.buffer().context("バッファの取得に失敗しました")?;
                let caps = sample.caps().context("capsの取得に失敗しました")?;

                let video_info = gstreamer_video::VideoInfo::from_caps(caps)
                    .context("VideoInfoの作成に失敗しました")?;

                let map = buffer.map_readable().context("バッファのマップに失敗しました")?;

                let current_frame = frame_count;
                frame_count += 1;

                if current_frame % self.config.frame_interval == 0 {
                    let width = video_info.width() as u32;
                    let height = video_info.height() as u32;

                    let contiguous = plane_to_contiguous_rgb(&video_info, map.as_slice());
                    let img = image::RgbImage::from_raw(width, height, contiguous)
                        .context("RgbImageの作成に失敗しました")?;

                    // コールバックを同期的に呼び出し（同じスレッド内）
                    callback(&img, current_frame)?;

                    processed_count += 1;

                    if processed_count % 30 == 0 {
                        println!("処理済み: {}フレーム", processed_count);
                    }
                }
            }
        }

        pipeline.set_state(gst::State::Null)
            .context("パイプラインの停止に失敗しました")?;

        println!("\n処理完了!");
        println!("  総フレーム数: {}", frame_count);
        println!("  処理フレーム数: {}", processed_count);

        Ok(())
    }

    /// 特定のフレーム番号のフレームをメモリ上で抽出（ファイル保存なし）
    pub fn extract_frame_to_memory<P: AsRef<Path>>(
        &self,
        video_path: P,
        frame_number: u32,
    ) -> Result<image::RgbImage> {
        gst::init()?;

        let pipeline = gst::Pipeline::default();

        let src = ElementFactory::make("filesrc")
            .name("src")
            .property("location", video_path.as_ref().to_str().unwrap())
            .build()?;

        let decodebin = ElementFactory::make("decodebin")
            .name("decoder")
            .build()?;
        
        let videoconvert = ElementFactory::make("videoconvert")
            .name("converter")
            .build()?;
        
        let videoscale = ElementFactory::make("videoscale")
            .name("scaler")
            .build()?;

        let appsink = AppSink::builder()
            .name("sink")
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "RGB")
                    .build(),
            )
            .build();

        pipeline.add_many([&src, &decodebin, &videoconvert, &videoscale, appsink.upcast_ref()])?;
        src.link(&decodebin)?;
        videoconvert.link(&videoscale)?;
        videoscale.link(&appsink)?;

        let videoconvert_weak = videoconvert.downgrade();
        decodebin.connect_pad_added(move |_, src_pad| {
            let Some(videoconvert) = videoconvert_weak.upgrade() else {
                return;
            };

            let sink_pad = videoconvert.static_pad("sink").expect("sink pad");
            if sink_pad.is_linked() {
                return;
            }

            if let Err(e) = src_pad.link(&sink_pad) {
                eprintln!("Failed to link pads: {}", e);
            }
        });

        pipeline.set_state(gst::State::Playing)?;

        let bus = pipeline.bus().unwrap();
        let mut frame_count = 0u32;
        let mut result_image: Option<image::RgbImage> = None;
        
        // タイムアウトを設定（10秒）
        let timeout = std::time::Duration::from_secs(10);
        let start_time = std::time::Instant::now();

        'outer: loop {
            // タイムアウトチェック
            if start_time.elapsed() > timeout {
                pipeline.set_state(gst::State::Null)?;
                return Err(anyhow::anyhow!("フレーム抽出がタイムアウトしました"));
            }

            // バスメッセージを処理
            while let Some(msg) = bus.pop() {
                use gst::MessageView;

                match msg.view() {
                    MessageView::Eos(..) => {
                        break 'outer;
                    }
                    MessageView::Error(err) => {
                        pipeline.set_state(gst::State::Null)?;
                        return Err(anyhow::anyhow!(
                            "エラー: {} (デバッグ: {:?})",
                            err.error(),
                            err.debug()
                        ));
                    }
                    _ => {}
                }
            }

            // フレームを取得
            if let Some(sample) = appsink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
                if frame_count == frame_number {
                    // 目的のフレームを取得
                    let buffer = sample.buffer().ok_or_else(|| anyhow::anyhow!("バッファなし"))?;
                    let caps = sample.caps().ok_or_else(|| anyhow::anyhow!("キャプスなし"))?;
                    let video_info = gstreamer_video::VideoInfo::from_caps(caps)?;

                    let map = buffer.map_readable().map_err(|_| anyhow::anyhow!("マップ失敗"))?;
                    let width = video_info.width();
                    let height = video_info.height();

                    let contiguous = plane_to_contiguous_rgb(&video_info, map.as_slice());
                    if let Some(img) = image::RgbImage::from_raw(width, height, contiguous) {
                        result_image = Some(img);
                        break 'outer;
                    }
                }
                frame_count += 1;
            }

            // CPU使用率を下げるため少し待機
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // パイプラインを確実に停止・解放
        pipeline.set_state(gst::State::Null)?;
        
        // 少し待機してリソースを解放
        std::thread::sleep(std::time::Duration::from_millis(100));

        result_image.ok_or_else(|| anyhow::anyhow!("指定されたフレームが見つかりませんでした"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_extractor_config_default() {
        let config = FrameExtractorConfig::default();
        assert_eq!(config.frame_interval, 1);
        assert_eq!(config.image_format, "png");
        assert_eq!(config.jpeg_quality, 95);
    }
}
