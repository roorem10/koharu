use koharu_pipeline::{DetectionModel, InpaintingModel, OcrModel, PipelineConfig, Stage};
use koharu_runtime::HuggingFaceFile;

/// The same pinned assets used by inference, without initializing a device or a model.
pub(super) struct ModelDownload {
    pub stage: Stage,
    pub name: &'static str,
    pub files: &'static [HuggingFaceFile<'static>],
}

pub(super) fn selected(pipeline: &PipelineConfig) -> [ModelDownload; 3] {
    let (name, files) = match &pipeline.detection {
        DetectionModel::KoharuLayoutRFDetrSeg2XL(_) => (
            "koharu-layout-rfdetr-seg-2xl",
            koharu_ml::koharu_layout_rfdetr_seg_2xl::MODEL_FILES,
        ),
    };
    let detection = ModelDownload {
        stage: Stage::Detection,
        name,
        files,
    };
    let (name, files) = match &pipeline.ocr {
        OcrModel::PaddleOcrVl1_6 => (
            "paddleocr-vl-1.6",
            koharu_ml::paddle_ocr_vl_quantized::MODEL_FILES,
        ),
        OcrModel::MangaOcr => ("manga-ocr", koharu_ml::manga_ocr::MODEL_FILES),
        OcrModel::BaberuOcr => ("baberu-ocr", koharu_ml::baberu_ocr::MODEL_FILES),
        OcrModel::HayaiOcr => ("hayai-ocr", koharu_ml::hayai_ocr::MODEL_FILES),
    };
    let ocr = ModelDownload {
        stage: Stage::Ocr,
        name,
        files,
    };
    let (name, files) = match &pipeline.inpainting {
        InpaintingModel::LaMa {} => ("lama", koharu_ml::lama::MODEL_FILES),
        InpaintingModel::AotInpainting {} => {
            ("aot-inpainting", koharu_ml::aot_inpainting::MODEL_FILES)
        }
        InpaintingModel::Flux2Klein(_) => ("flux2-klein", koharu_ml::flux2_klein::MODEL_FILES),
        InpaintingModel::RoremMixed(_) => ("rorem-mixed", koharu_ml::rorem_mixed::MODEL_FILES),
    };
    [
        detection,
        ocr,
        ModelDownload {
            stage: Stage::Inpainting,
            name,
            files,
        },
    ]
}
