use super::{
    requirements::{MemoryRequirement, ModelRequirements},
    ModelFile,
};

pub(super) struct ModelSource {
    pub id: &'static str,
    pub name: &'static str,
    pub base: &'static str,
}

pub(super) struct ModelDefinition {
    pub id: &'static str,
    pub revision: &'static str,
    pub name: &'static str,
    pub publisher: &'static str,
    pub description: &'static str,
    pub capability: &'static str,
    pub requirements: ModelRequirements,
    pub sources: &'static [ModelSource],
}

pub(super) const OCR: ModelDefinition = ModelDefinition {
    id: "minicpm-ocr-1.0",
    revision: "857af228a0b3cc79588bb90635bb6c00dd9542da",
    name: "MiniCPM-OCR 1.0",
    publisher: "OpenBMB / 面壁智能",
    description: "中英文文档识别，支持文字、表格、公式和图表。",
    capability: "ocr",
    requirements: ModelRequirements {
        os: "windows",
        arch: "x86_64",
        platform_name: "Windows x64 · CPU 推理",
        // b11901、上下文 8192 的基础 OCR 实测私有内存峰值约 2.00 GiB，含约 0.84 GiB 视觉权重。
        // 按完整权重加 1.25 GiB 运行缓冲、0.25 GiB 余量提示，不限制下载。
        memory_requirement: Some(MemoryRequirement {
            available_bytes: 529_106_816 + 901_017_536 + 1536 * 1024 * 1024,
            estimated: true,
            source: "CPU / 上下文 8192；完整权重 + 1.25 GiB 运行缓冲 + 0.25 GiB 余量",
        }),
        // 大小和 SHA-256 来自 OpenBMB 官方仓库，语言权重与视觉模块必须配套安装。
        files: &[
            ModelFile {
                name: "MiniCPM-OCR-1_0-Q4_K_M.gguf",
                bytes: 529_106_816,
                sha256: "c80a703400633f298cc2fddf6d493a541be36918c0801c0b1bf3f14c5703ce49",
            },
            ModelFile {
                name: "mmproj-model-f16-4x.gguf",
                bytes: 901_017_536,
                sha256: "ceed82261b3b1380c3165f8b37c10117a50093be1362cbfeeb692fa036444682",
            },
        ],
    },
    sources: &[
        ModelSource {
            id: "modelscope",
            name: "ModelScope（官方）",
            base: "https://modelscope.cn/models/OpenBMB/MiniCPM-OCR-1.0-GGUF/resolve/857af228a0b3cc79588bb90635bb6c00dd9542da",
        },
        ModelSource {
            id: "huggingface",
            name: "Hugging Face（官方）",
            base: "https://huggingface.co/openbmb/MiniCPM-OCR-1.0-gguf/resolve/main",
        },
    ],
};

// 原模型：https://huggingface.co/NiuTrans/LMT-60-0.6B
// 社区量化：https://huggingface.co/mradermacher/LMT-60-0.6B-GGUF
// 文件大小、SHA-256 和 revision 核对自仓库 API；镜像只作为同一文件的下载渠道。
pub(super) const LMT: ModelDefinition = ModelDefinition {
    id: "lmt-60-0.6b",
    revision: "86942b9f925bfccd4196d4982578b46413a1f73f",
    name: "LMT-60-0.6B",
    publisher: "NiuTrans / 东北大学 · mradermacher 社区量化",
    description: "专用本地翻译模型，覆盖 60 种语言、234 个中英中心翻译方向。",
    capability: "translation",
    requirements: ModelRequirements {
        os: "windows",
        arch: "x86_64",
        platform_name: "Windows x64 · CPU 推理",
        // Qwen3 的 28 层、8 个 KV 头、head_dim 128，8192 上下文的 F16 KV 约 896 MiB。
        // 另留 512 MiB 计算缓冲和 256 MiB 余量；仅作运行参考，不限制下载。
        memory_requirement: Some(MemoryRequirement {
            available_bytes: 484_220_000 + (896 + 512 + 256) * 1024 * 1024,
            estimated: true,
            source: "CPU / 上下文 8192；Q4_K_M 权重 + 896 MiB KV 缓存 + 512 MiB 计算缓冲 + 256 MiB 余量",
        }),
        files: &[ModelFile {
            name: "LMT-60-0.6B.Q4_K_M.gguf",
            bytes: 484_220_000,
            sha256: "743c6cdc13294b5c470ec0e89e79d64f96ef0cab94ce1285c78da4dc0f56be5c",
        }],
    },
    sources: &[
        ModelSource {
            id: "hf-mirror",
            name: "HF Mirror（社区量化镜像）",
            base: "https://hf-mirror.com/mradermacher/LMT-60-0.6B-GGUF/resolve/86942b9f925bfccd4196d4982578b46413a1f73f",
        },
        ModelSource {
            id: "huggingface",
            name: "Hugging Face（社区量化）",
            base: "https://huggingface.co/mradermacher/LMT-60-0.6B-GGUF/resolve/86942b9f925bfccd4196d4982578b46413a1f73f",
        },
    ],
};

pub(super) const MODELS: &[ModelDefinition] = &[OCR, LMT];

pub(super) fn find(id: &str) -> Result<&'static ModelDefinition, super::ManagerError> {
    MODELS
        .iter()
        .find(|model| model.id == id)
        .ok_or_else(|| super::failure("不支持的本地模型"))
}
