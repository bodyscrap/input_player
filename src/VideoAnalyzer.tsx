import { useState, useRef, useEffect } from "react";
import "./VideoAnalyzer.css";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface VideoInfo {
  width: number;
  height: number;
  fps: number;
  duration_sec: number;
}

interface AnalysisRegion {
  x: number;
  y: number;
  tile_size: number;
  cols: number;
}

interface VideoAnalyzerProps {
  onClose: () => void;
  initialStep?: "region-setup" | "collect-data";
}

export default function VideoAnalyzer({ onClose, initialStep = "region-setup" }: VideoAnalyzerProps) {
  // ワークフロー: region-config → tile-extract → collect-data → (manual-labeling) → train → inference
  const [currentStep, _setCurrentStep] = useState<"region-config" | "tile-extract" | "collect-data" | "inference">(
    initialStep === "region-setup" ? "region-config" : "collect-data"
  );
  
  // デバッグ用
  useEffect(() => {
    console.log("VideoAnalyzer mounted with:", { initialStep, currentStep });
  }, []);
  
  const [videoPath, setVideoPath] = useState<string>("");
  const [videoInfo, setVideoInfo] = useState<VideoInfo | null>(null);
  const [region, setRegion] = useState<AnalysisRegion>({
    x: 80,
    y: 400,
    tile_size: 80,
    cols: 6,
  });
  const [frameInterval, setFrameInterval] = useState<number>(10); // タイル抽出時の間引き間隔
  const [tileOutputDir, setTileOutputDir] = useState<string>("");
  const [isProcessing, setIsProcessing] = useState(false);
  const [progress, setProgress] = useState<string>("");
  const [zoom, setZoom] = useState<number>(1.0);
  const [panX, setPanX] = useState<number>(0);
  const [panY, setPanY] = useState<number>(0);
  const [isDragging, setIsDragging] = useState<boolean>(false);
  const [dragStart, setDragStart] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [currentFrame, setCurrentFrame] = useState<number>(0);
  const videoRef = useRef<HTMLVideoElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const isFirstLoadRef = useRef<boolean>(true); // 初回読み込みフラグ
  
  // 最新のzoom/pan値をrefで保持
  const zoomRef = useRef<number>(zoom);
  const panXRef = useRef<number>(panX);
  const panYRef = useRef<number>(panY);
  
  // refを常に最新に保つ
  useEffect(() => {
    zoomRef.current = zoom;
    panXRef.current = panX;
    panYRef.current = panY;
  }, [zoom, panX, panY]);

  // モーダル表示中はbodyのスクロールを無効化
  useEffect(() => {
    // モーダルが開いている間、bodyのスクロールを無効化
    const originalOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    
    return () => {
      // クリーンアップ: 元のスタイルに戻す
      document.body.style.overflow = originalOverflow;
    };
  }, []);

  // preview-containerでホイールイベントをpassive: falseで登録
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const handleWheelNative = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      
      if (!videoRef.current || !canvasRef.current) return;
      
      // refから最新の値を取得
      const currentZoom = zoomRef.current;
      const currentPanX = panXRef.current;
      const currentPanY = panYRef.current;
      
      // 直接ネイティブイベントで処理
      const canvas = canvasRef.current;
      const rect = canvas.getBoundingClientRect();
      const minZoom = 1.0;
      const maxZoom = canvas.width / rect.width;
      
      const delta = e.deltaY > 0 ? 0.9 : 1.1;
      const newZoom = Math.max(minZoom, Math.min(maxZoom, currentZoom * delta));
      
      // マウス位置を中心にズーム
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;
      
      const scaleX = canvas.width / rect.width;
      const scaleY = canvas.height / rect.height;
      
      const canvasMouseX = mouseX * scaleX;
      const canvasMouseY = mouseY * scaleY;
      
      const worldX = (canvasMouseX - currentPanX) / currentZoom;
      const worldY = (canvasMouseY - currentPanY) / currentZoom;
      
      const newPanX = canvasMouseX - worldX * newZoom;
      const newPanY = canvasMouseY - worldY * newZoom;
      
      setPanX(newPanX);
      setPanY(newPanY);
      setZoom(newZoom);
    };

    // passive: false を明示的に指定
    container.addEventListener('wheel', handleWheelNative, { passive: false });

    return () => {
      container.removeEventListener('wheel', handleWheelNative);
    };
  }, [videoPath, videoInfo]); // 動画が読み込まれた後に登録

  // コンポーネントマウント時に保存された設定を読み込む
  useEffect(() => {
    const loadSavedRegion = async () => {
      try {
        const savedRegion = await invoke<{
          x: number;
          y: number;
          tile_width: number;
          tile_height: number;
          columns: number;
          rows: number;
        }>("load_analysis_region");
        
        // バックエンド形式からフロントエンド形式に変換
        setRegion({
          x: savedRegion.x,
          y: savedRegion.y,
          tile_size: savedRegion.tile_width,
          cols: savedRegion.columns,
        });
        console.log("保存された解析範囲を読み込みました:", savedRegion);
      } catch (error) {
        console.log("保存された解析範囲がありません（初回起動）");
      }
    };
    loadSavedRegion();
  }, []);

  // キャンバスに矩形を描画（region変更時に自動更新）
  useEffect(() => {
    if (currentStep === "region-config" && videoRef.current) {
      drawRegionOnCanvas();
    }
  }, [region, currentStep]);
  
  // zoom/pan変更時も再描画
  useEffect(() => {
    if (currentStep === "region-config" && videoRef.current) {
      drawRegionOnCanvas();
    }
  }, [zoom, panX, panY]);

  // videoInfoが設定された後に動画を読み込む
  useEffect(() => {
    const loadVideo = async () => {
      if (!videoPath || !videoInfo || !videoRef.current) {
        console.log("動画読み込みスキップ:", { hasVideoPath: !!videoPath, hasVideoInfo: !!videoInfo, hasVideoRef: !!videoRef.current });
        return;
      }

      console.log("videoタグに動画を設定します...");
      const video = videoRef.current;
      
      // エラーハンドラー
      video.onerror = (e) => {
        console.error("Video読み込みエラー:", e);
        alert("動画の読み込みに失敗しました。ファイル形式を確認してください。");
      };
      
      // メタデータ読み込み後にキャンバスを初期化（初回のみ）
      video.onloadedmetadata = () => {
        console.log("Video metadata loaded:", {
          width: video.videoWidth,
          height: video.videoHeight,
          duration: video.duration,
          isFirstLoad: isFirstLoadRef.current
        });
        
        if (video.videoWidth > 0 && video.videoHeight > 0 && containerRef.current && canvasRef.current) {
          video.pause();
          
          // キャンバスのサイズを動画に合わせて設定
          const canvas = canvasRef.current;
          canvas.width = video.videoWidth;
          canvas.height = video.videoHeight;
          
          // 初回読み込み時のみzoom/panを初期化
          if (isFirstLoadRef.current) {
            video.currentTime = 0;
            
            // 初期ズームを1.0に設定（CSS表示領域にフィット）
            const rect = canvas.getBoundingClientRect();
            const cssScale = rect.width / canvas.width;
            console.log("CSS scale:", cssScale, "Canvas:", canvas.width, "Display:", rect.width);
            console.log("Initial zoom set to 1.0 (fit to display)");
            
            // zoom=1.0がCSS表示領域にフィット
            setZoom(1.0);
            setPanX(0);
            setPanY(0);
            setCurrentFrame(0);
            isFirstLoadRef.current = false;
          }
        }
      };
      
      // データ読み込み完了後に描画
      video.onloadeddata = () => {
        console.log("Video data loaded, ready to draw");
        if (video.readyState >= 2 && canvasRef.current) { // HAVE_CURRENT_DATA
          video.pause();
          
          // キャンバスサイズを確認・設定
          const canvas = canvasRef.current;
          if (canvas.width !== video.videoWidth || canvas.height !== video.videoHeight) {
            canvas.width = video.videoWidth;
            canvas.height = video.videoHeight;
            console.log("Canvas size set to:", canvas.width, "x", canvas.height);
          }
          
          // 少し待ってから描画（フレームが確実に準備されるまで）
          setTimeout(() => {
            console.log("Drawing canvas...");
            drawRegionOnCanvas();
          }, 100);
        }
      };
      
      // フレーム更新時にキャンバスを再描画
      video.onseeked = () => {
        if (videoInfo) {
          setCurrentFrame(Math.floor(video.currentTime * videoInfo.fps));
        }
        drawRegionOnCanvas();
      };
      
      // Tauriの asset プロトコルで動画を読み込む
      try {
        const { convertFileSrc } = await import("@tauri-apps/api/core");
        const videoSrc = convertFileSrc(videoPath);
        console.log("Loading video from:", videoSrc);
        video.src = videoSrc;
        video.load();
      } catch (error) {
        console.error("convertFileSrcエラー:", error);
        alert(`動画URLの変換に失敗しました: ${error}`);
      }
    };

    loadVideo();
  }, [videoPath, videoInfo]);

  // 動画ファイル選択
  const handleSelectVideo = async () => {
    try {
      console.log("動画選択ダイアログを開きます...");
      const selected = await open({
        multiple: false,
        filters: [{
          name: "Video",
          extensions: ["mp4", "avi", "mkv", "mov"]
        }]
      });

      console.log("選択された動画:", selected);

      if (selected) {
        setVideoPath(selected);
        
        console.log("動画情報を取得中...");
        // 動画情報取得
        const info = await invoke<VideoInfo>("get_video_info", { videoPath: selected });
        console.log("動画情報取得完了:", info);
        setVideoInfo(info);
        
        // 新しい動画を読み込む際は初回フラグをリセット
        isFirstLoadRef.current = true;
        
        // currentStepは変更しない（学習データ収集画面から動画を選択した場合も維持）
        // 動画の読み込みはuseEffectで自動的に行われる
      }
    } catch (error) {
      console.error("動画選択エラー:", error);
      alert(`動画の読み込みに失敗しました: ${error}`);
    }
  };

  // 動画の時間を変更
  const handleSeekVideo = (frameNum: number) => {
    if (!videoRef.current || !videoInfo) return;
    const time = frameNum / videoInfo.fps;
    videoRef.current.currentTime = time;
    setCurrentFrame(frameNum);
  };

  // 領域設定を保存して次へ
  const handleSaveRegion = async () => {
    try {
      // 動画の解像度情報を含めて保存
      const regionToSave = {
        x: region.x,
        y: region.y,
        tile_width: region.tile_size,
        tile_height: region.tile_size,
        columns: region.cols,
        rows: 1,
        video_width: videoInfo?.width || 1920,
        video_height: videoInfo?.height || 1080,
      };
      await invoke("save_analysis_region", { region: regionToSave });
      alert("入力解析範囲を保存しました");
      onClose(); // 設定完了したら閉じる
    } catch (error) {
      console.error("領域保存エラー:", error);
      alert(`領域設定の保存に失敗しました: ${error}`);
    }
  };

  // 学習データ収集（GStreamer AppSinkを使用）
  const handleCollectTrainingData = async () => {
    if (!tileOutputDir) {
      alert("出力先ディレクトリを選択してください");
      return;
    }

    if (!videoPath) {
      alert("動画ファイルを選択してください");
      return;
    }

    setIsProcessing(true);
    setProgress("学習データを収集中...");

    try {
      // バックエンドが期待する形式に変換
      const regionToSend = {
        x: region.x,
        y: region.y,
        tile_width: region.tile_size,
        tile_height: region.tile_size,
        columns: region.cols,
        rows: 1,
      };

      const result = await invoke<{ tile_count: number; frame_count: number; message: string }>("collect_training_data", {
        videoPath,
        outputDir: tileOutputDir,
        frameInterval,
        region: regionToSend,
      });

      // デフォルトフォルダ作成の確認
      const createFolders = window.confirm(
        `${result.message}\n出力先: ${tileOutputDir}\n\n抽出されたフレーム数: ${result.frame_count}\nタイル総数: ${result.tile_count}\n\n次のステップ:\n1. 抽出された画像を確認\n2. クラスごとにフォルダ分け\n3. モデル学習を実行\n\nデフォルトの分類フォルダ（dir_1～dir_9, others）を作成しますか？`
      );

      if (createFolders) {
        try {
          await invoke("create_default_classification_folders", {
            outputDir: tileOutputDir,
          });
          alert("デフォルトフォルダを作成しました。\n\ndir_1, dir_2, dir_3, dir_4, dir_6, dir_7, dir_8, dir_9, others");
        } catch (error) {
          console.error("フォルダ作成エラー:", error);
          alert(`フォルダの作成に失敗しました: ${error}`);
        }
      }

      onClose();
    } catch (error) {
      console.error("学習データ収集エラー:", error);
      const errorMessage = String(error);
      if (errorMessage.includes("GStreamer")) {
        alert(`学習データ収集に失敗しました: ${error}\n\nGStreamerが正しくインストールされているか確認してください。\nhttps://gstreamer.freedesktop.org/download/`);
      } else {
        alert(`学習データ収集に失敗しました: ${error}`);
      }
    } finally {
      setIsProcessing(false);
      setProgress("");
    }
  };

  // タイル抽出（学習データ準備）- 旧実装
  const handleExtractTiles = async () => {
    if (!tileOutputDir) {
      alert("出力先ディレクトリを選択してください");
      return;
    }

    setIsProcessing(true);
    setProgress("タイル画像を抽出中...");

    try {
      const result = await invoke<{ tile_count: number; message: string }>("extract_tiles_from_video", {
        videoPath,
        outputDir: tileOutputDir,
        frameInterval,
        region,
      });

      alert(`${result.message}\n出力先: ${tileOutputDir}\n\n次のステップ:\n1. 抽出された画像を確認\n2. クラスごとにフォルダ分け\n3. モデル学習を実行`);
      onClose();
    } catch (error) {
      console.error("タイル抽出エラー:", error);
      alert(`タイル抽出に失敗しました: ${error}`);
    } finally {
      setIsProcessing(false);
      setProgress("");
    }
  };

  // キャンバスに矩形を描画
  const drawRegionOnCanvas = () => {
    if (!canvasRef.current || !videoRef.current) return;

    const canvas = canvasRef.current;
    const video = videoRef.current;
    
    // videoが準備できていない場合はスキップ
    if (video.videoWidth === 0 || video.videoHeight === 0) return;
    
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // videoのサイズに合わせてcanvasのサイズを設定（変更がある場合のみ）
    if (canvas.width !== video.videoWidth || canvas.height !== video.videoHeight) {
      canvas.width = video.videoWidth;
      canvas.height = video.videoHeight;
    }

    // 変換をリセット
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    
    // キャンバスをクリア（前の描画を消す）
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // ズームとパンを適用（平行移動 → スケールの順序）
    // refから最新の値を取得
    const currentZoom = zoomRef.current;
    const currentPanX = panXRef.current;
    const currentPanY = panYRef.current;
    
    ctx.save();
    ctx.translate(currentPanX, currentPanY);
    ctx.scale(currentZoom, currentZoom);

    // videoの現在のフレームを描画（video本来のサイズで描画、scaleで縮小される）
    ctx.drawImage(video, 0, 0, video.videoWidth, video.videoHeight);

    // 領域の矩形を描画（計算された幅・高さを使用）
    const regionWidth = region.tile_size * region.cols;
    const regionHeight = region.tile_size;
    ctx.strokeStyle = "#00ff00";
    ctx.lineWidth = 3 / currentZoom; // ズームレベルに応じて線の太さを調整
    ctx.strokeRect(region.x, region.y, regionWidth, regionHeight);

    // タイルのグリッドを描画（1行のみ、正方形タイル）
    ctx.strokeStyle = "#ffff00";
    ctx.lineWidth = 1 / currentZoom; // ズームレベルに応じて線の太さを調整

    for (let col = 0; col < region.cols; col++) {
      const x = region.x + col * region.tile_size;
      const y = region.y;
      ctx.strokeRect(x, y, region.tile_size, region.tile_size);
    }
    
    ctx.restore();
  };



  // ドラッグでパン
  const handleMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button === 0 && canvasRef.current) { // 左クリック
      const canvas = canvasRef.current;
      const rect = canvas.getBoundingClientRect();
      
      // CSS座標での位置（ピクセル値）
      const startX = e.clientX - rect.left;
      const startY = e.clientY - rect.top;
      
      setIsDragging(true);
      setDragStart({ x: startX, y: startY });
    }
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDragging && canvasRef.current) {
      const canvas = canvasRef.current;
      const rect = canvas.getBoundingClientRect();
      
      // CSS座標での現在位置
      const currentX = e.clientX - rect.left;
      const currentY = e.clientY - rect.top;
      
      // CSS座標での移動量
      const deltaX = currentX - dragStart.x;
      const deltaY = currentY - dragStart.y;
      
      // CSS座標からキャンバス座標への変換
      const scaleX = canvas.width / rect.width;
      const scaleY = canvas.height / rect.height;
      
      // pan値を更新（キャンバス座標系での移動量）
      setPanX(panX + deltaX * scaleX);
      setPanY(panY + deltaY * scaleY);
      
      // 次のフレームのために開始位置を更新
      setDragStart({ x: currentX, y: currentY });
    }
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  // ズームリセット（フィットサイズに戻す）
  const resetZoom = () => {
    if (!videoRef.current || !canvasRef.current) return;
    const canvas = canvasRef.current;
    if (canvas.width === 0) return;
    
    console.log("resetZoom:", { 
      canvasWidth: canvas.width, 
      currentZoom: zoom 
    });
    
    // zoom=1.0でCSS表示領域にフィット
    setZoom(1.0);
    setPanX(0);
    setPanY(0);
  };

  // ズームイン
  const zoomIn = () => {
    if (!videoRef.current || !containerRef.current || !canvasRef.current) return;
    const canvas = canvasRef.current;
    const rect = canvas.getBoundingClientRect();
    // 最大ズームは実サイズ（CSS表示サイズの何倍か）
    const maxZoom = canvas.width / rect.width;
    const newZoom = Math.min(maxZoom, zoom * 1.2);
    
    // キャンバスピクセル座標の中心
    const canvasCenterX = canvas.width / 2;
    const canvasCenterY = canvas.height / 2;
    
    console.log("zoomIn:", { 
      currentZoom: zoom, 
      newZoom, 
      canvasWidth: canvas.width,
      canvasHeight: canvas.height,
      canvasCenterX, 
      canvasCenterY, 
      panX, 
      panY 
    });
    
    // キャンバス中心座標に対応するワールド座標を逆変換で求める
    const worldX = (canvasCenterX - panX) / zoom;
    const worldY = (canvasCenterY - panY) / zoom;
    
    // ズーム後、同じワールド座標がキャンバス中心に来るようにパンを調整
    const newPanX = canvasCenterX - worldX * newZoom;
    const newPanY = canvasCenterY - worldY * newZoom;
    
    console.log("zoomIn after:", { worldX, worldY, newPanX, newPanY });
    
    setPanX(newPanX);
    setPanY(newPanY);
    setZoom(newZoom);
  };

  // ズームアウト
  const zoomOut = () => {
    if (!videoRef.current || !containerRef.current || !canvasRef.current) return;
    const canvas = canvasRef.current;
    if (canvas.width === 0) return;
    
    // 最小ズームは1.0（CSS表示領域にフィット）
    const minZoom = 1.0;
    const newZoom = Math.max(minZoom, zoom * 0.8);
    
    // キャンバスピクセル座標の中心
    const canvasCenterX = canvas.width / 2;
    const canvasCenterY = canvas.height / 2;
    
    console.log("zoomOut:", { 
      currentZoom: zoom, 
      minZoom, 
      newZoom, 
      canvasWidth: canvas.width,
      canvasHeight: canvas.height,
      canvasCenterX,
      canvasCenterY,
      panX, 
      panY 
    });
    
    // キャンバス中心座標に対応するワールド座標を逆変換で求める
    const worldX = (canvasCenterX - panX) / zoom;
    const worldY = (canvasCenterY - panY) / zoom;
    
    // ズーム後、同じワールド座標がキャンバス中心に来るようにパンを調整
    const newPanX = canvasCenterX - worldX * newZoom;
    const newPanY = canvasCenterY - worldY * newZoom;
    
    console.log("zoomOut after:", { worldX, worldY, newPanX, newPanY });
    
    setPanX(newPanX);
    setPanY(newPanY);
    setZoom(newZoom);
  };

  return (
    <div className="video-analyzer-overlay" onClick={onClose}>
      <div 
        className="video-analyzer-modal" 
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-header">
          <h2>{currentStep === "collect-data" ? "学習データ収集" : "解析範囲設定"}</h2>
          <button onClick={onClose} className="close-button">×</button>
        </div>

        <div className="modal-body">
          {/* ステップ1: 動画選択 */}
          {currentStep === "region-config" && !videoInfo && (
            <div className="step-content">
              <p>動画を選択して入力インジケータの位置とサイズを設定します。</p>
              <button onClick={handleSelectVideo} className="primary-button">
                動画を選択
              </button>
            </div>
          )}

          {/* ステップ2: 領域設定（プレビュー表示後） */}
          {currentStep === "region-config" && videoInfo && (
            <div className="step-content">
              <div className="video-info">
                <p>動画: {videoPath}</p>
                <p>解像度: {videoInfo.width}x{videoInfo.height}</p>
                <p>FPS: {videoInfo.fps.toFixed(2)}</p>
                <p>再生時間: {videoInfo.duration_sec.toFixed(2)}秒</p>
              </div>

              <div 
                className="preview-container" 
                style={{ position: "relative", overflow: "hidden", background: "#000" }}
                ref={containerRef}
                onMouseDown={handleMouseDown}
                onMouseMove={handleMouseMove}
                onMouseUp={handleMouseUp}
                onMouseLeave={handleMouseUp}
              >
                {/* 非表示のvideoタグ */}
                <video
                  ref={videoRef}
                  style={{ display: "none" }}
                  preload="auto"
                  crossOrigin="anonymous"
                  muted
                  playsInline
                />
                {/* 描画用canvas */}
                <canvas
                  ref={canvasRef}
                  style={{ 
                    width: "100%",
                    height: "auto",
                    display: "block",
                    cursor: isDragging ? "grabbing" : "grab",
                    userSelect: "none"
                  }}
                />
                {/* ズームコントロール */}
                <div style={{
                  position: "absolute",
                  top: "10px",
                  right: "10px",
                  display: "flex",
                  flexDirection: "column",
                  gap: "5px",
                  background: "rgba(0, 0, 0, 0.7)",
                  padding: "10px",
                  borderRadius: "5px"
                }}>
                  <button 
                    onClick={(e) => { e.stopPropagation(); zoomIn(); }}
                    className="secondary-button"
                    style={{ padding: "5px 10px", minWidth: "40px" }}
                    title="拡大"
                  >
                    ➕
                  </button>
                  <div style={{ color: "#fff", textAlign: "center", fontSize: "12px" }}>
                    {(zoom * 100).toFixed(0)}%
                  </div>
                  <button 
                    onClick={(e) => { e.stopPropagation(); zoomOut(); }}
                    className="secondary-button"
                    style={{ padding: "5px 10px", minWidth: "40px" }}
                    title="縮小"
                  >
                    ➖
                  </button>
                  <button 
                    onClick={(e) => { e.stopPropagation(); resetZoom(); }}
                    className="secondary-button"
                    style={{ padding: "5px 10px", fontSize: "10px" }}
                    title="フィットサイズにリセット"
                  >
                    FIT
                  </button>
                </div>
              </div>

              <div className="region-settings">
                <label style={{ gridColumn: "1 / -1" }}>
                  プレビューフレーム:
                  <input 
                    type="range" 
                    min={0} 
                    max={videoInfo ? Math.floor(videoInfo.duration_sec * videoInfo.fps) - 1 : 0}
                    value={currentFrame}
                    onChange={(e) => handleSeekVideo(Number(e.target.value))}
                    onMouseDown={(e) => e.stopPropagation()}
                    style={{ width: "100%" }}
                  />
                  <span style={{ marginLeft: "10px" }}>
                    フレーム: {currentFrame} / {Math.floor(videoInfo.duration_sec * videoInfo.fps)}
                  </span>
                </label>
                <label>
                  X座標: <input type="number" value={region.x} onChange={(e) => setRegion({...region, x: Number(e.target.value)})} />
                </label>
                <label>
                  Y座標: <input type="number" value={region.y} onChange={(e) => setRegion({...region, y: Number(e.target.value)})} />
                </label>
                <label>
                  タイルサイズ（正方形）: <input type="number" value={region.tile_size} onChange={(e) => setRegion({...region, tile_size: Number(e.target.value)})} />
                  <span style={{ fontSize: "12px", color: "#ccc", marginLeft: "10px" }}>（幅 = {region.tile_size * region.cols}, 高さ = {region.tile_size}）</span>
                </label>
                <label>
                  列数（ボタン数）: <input type="number" value={region.cols} min={1} onChange={(e) => setRegion({...region, cols: Number(e.target.value)})} />
                </label>
              </div>

              <div style={{ marginTop: "20px", padding: "10px", backgroundColor: "#2a2a2a", border: "1px solid #4a9eff", borderRadius: "5px" }}>
                <p style={{ color: "#4a9eff", margin: "0 0 10px 0" }}><strong>💡 ヒント:</strong></p>
                <ul style={{ marginLeft: "20px", fontSize: "14px", color: "#ccc" }}>
                  <li>スライダーで確認したいフレームに移動できます</li>
                  <li><strong>マウスホイール</strong>でズーム、<strong>ドラッグ</strong>で画面移動できます</li>
                  <li>緑の枠が解析範囲全体です</li>
                  <li>黄色のグリッドが各ボタンタイルです</li>
                  <li>この設定は保存され、次回以降も使用されます</li>
                </ul>
              </div>

              <button onClick={handleSaveRegion} className="primary-button">
                設定を保存して閉じる
              </button>
            </div>
          )}

          {/* 学習データ収集 */}
          {currentStep === "collect-data" && (
            <div className="step-content">
              <h3>学習データ収集</h3>
              <p>設定した解析範囲から学習用タイル画像を直接抽出します</p>

              <div className="file-selection">
                <label>
                  動画ファイル:
                  <div className="file-input-group">
                    <input type="text" value={videoPath} readOnly placeholder="動画ファイルを選択" />
                    <button onClick={handleSelectVideo} className="secondary-button">選択</button>
                  </div>
                </label>

                <label>
                  出力先ディレクトリ:
                  <div className="file-input-group">
                    <input type="text" value={tileOutputDir} readOnly placeholder="タイル画像の保存先" />
                    <button onClick={async () => {
                      const selected = await open({ directory: true });
                      if (selected) setTileOutputDir(selected);
                    }} className="secondary-button">選択</button>
                  </div>
                </label>

                <label>
                  フレーム間引き間隔:
                  <input 
                    type="number" 
                    value={frameInterval} 
                    onChange={(e) => setFrameInterval(Number(e.target.value))} 
                    min={1}
                  />
                  <span style={{ fontSize: "12px", color: "#ccc", marginLeft: "10px" }}>（例: 30 = 30フレームごとに1枚抽出）</span>
                </label>
              </div>

              <div style={{ marginTop: "20px", padding: "10px", backgroundColor: "#2a2a2a", border: "1px solid #4a9eff", borderRadius: "5px" }}>
                <p style={{ color: "#4a9eff", margin: "0 0 10px 0" }}><strong>💡 ヒント:</strong></p>
                <ul style={{ marginLeft: "20px", fontSize: "14px", color: "#ccc" }}>
                  <li>保存された解析範囲を使用します</li>
                  <li>動画から直接フレームを取得するため高速です</li>
                  <li>ファイル名形式: ｛動画名｝_frame=｛フレーム番号｝_tile=｛タイル番号｝.png</li>
                  <li>抽出後、クラスごとにフォルダ分けしてください</li>
                </ul>
              </div>

              {progress && (
                <div className="progress-message">
                  {progress}
                </div>
              )}

              <button 
                onClick={handleCollectTrainingData} 
                className="primary-button"
                disabled={isProcessing || !tileOutputDir || !videoPath}
              >
                {isProcessing ? "収集中..." : "学習データを収集"}
              </button>
            </div>
          )}

          {/* タイル抽出（学習データ準備）- 旧実装 */}
          {currentStep === "tile-extract" && (
            <div className="step-content">
              <h3>学習用タイル画像の抽出（旧実装）</h3>
              <p>設定した範囲からタイル画像を大量に抽出します（モデル不要）</p>

              <div className="file-selection">
                <label>
                  出力先ディレクトリ:
                  <div className="file-input-group">
                    <input type="text" value={tileOutputDir} readOnly placeholder="タイル画像の保存先" />
                    <button onClick={async () => {
                      const selected = await open({ directory: true });
                      if (selected) setTileOutputDir(selected);
                    }} className="secondary-button">選択</button>
                  </div>
                </label>

                <label>
                  フレーム間引き間隔:
                  <input 
                    type="number" 
                    value={frameInterval} 
                    onChange={(e) => setFrameInterval(Number(e.target.value))} 
                    min={1}
                  />
                  <span style={{ fontSize: "12px", color: "#666" }}>（例: 30 = 30フレームごとに1枚抽出）</span>
                </label>
              </div>

              {progress && (
                <div className="progress-message">
                  {progress}
                </div>
              )}

              <button 
                onClick={handleExtractTiles} 
                className="primary-button"
                disabled={isProcessing || !tileOutputDir}
              >
                {isProcessing ? "抽出中..." : "タイル画像を抽出"}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
