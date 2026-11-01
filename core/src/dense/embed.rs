use std::path::PathBuf;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::{Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy};

use super::{DIM, MAX_TOKENS, fetch, fetch_hint, runtime_dir};

const PAD_ID: i64 = 50283;

pub struct Embedder {
    session: Session,
    tokenizer: Tokenizer,
    wants_types: bool,
    output: usize,
}

fn locate_runtime() -> PathBuf {
    if let Some(path) = std::env::var_os("ORT_DYLIB_PATH")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    let prefix = std::env::consts::DLL_PREFIX;
    let suffix = std::env::consts::DLL_SUFFIX;
    let stem = format!("{prefix}onnxruntime");
    if let Ok(entries) = std::fs::read_dir(runtime_dir()) {
        let mut found: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&stem) && n.contains(suffix.trim_start_matches('.')) && !n.contains("providers"))
            })
            .collect();
        found.sort();
        if let Some(first) = found.into_iter().next() {
            return first;
        }
    }
    PathBuf::from(format!("{stem}{suffix}"))
}

fn ort_err<T>(e: ort::Error<T>) -> String {
    format!("dense: onnxruntime: {e}")
}

impl Embedder {
    pub fn load() -> crate::Result<Embedder> {
        if !fetch::present() {
            return Err(fetch_hint());
        }
        let runtime = locate_runtime();
        let builder = ort::init_from(&runtime).map_err(|e| {
            format!(
                "dense: cannot load the ONNX Runtime library {} ({e}); install onnxruntime 1.23 or newer and set ORT_DYLIB_PATH to its shared library, or place it in {}",
                runtime.display(),
                runtime_dir().display()
            )
        })?;
        let environment = builder.with_name("formslot").build().map_err(ort_err)?;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        let session = Session::builder(&environment)
            .map_err(ort_err)?
            .with_intra_threads(threads)
            .map_err(ort_err)?
            .commit_from_file(fetch::model_file())
            .map_err(ort_err)?;
        let mut tokenizer =
            Tokenizer::from_file(fetch::tokenizer_file()).map_err(|e| format!("dense: tokenizer: {e}"))?;
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                strategy: TruncationStrategy::LongestFirst,
                stride: 0,
                direction: TruncationDirection::Right,
            }))
            .map_err(|e| format!("dense: tokenizer: {e}"))?;
        let wants_types = session.inputs().iter().any(|i| i.name() == "token_type_ids");
        let output = session
            .outputs()
            .iter()
            .position(|o| o.name() == "last_hidden_state")
            .unwrap_or(0);
        Ok(Embedder {
            session,
            tokenizer,
            wants_types,
            output,
        })
    }

    pub fn embed(&mut self, texts: &[String]) -> crate::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| format!("dense: tokenizer: {e}"))?;
        let rows = encodings.len();
        let width = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(1).max(1);
        let mut ids = vec![PAD_ID; rows * width];
        let mut mask = vec![0i64; rows * width];
        for (r, enc) in encodings.iter().enumerate() {
            for (c, id) in enc.get_ids().iter().enumerate() {
                ids[r * width + c] = i64::from(*id);
                mask[r * width + c] = 1;
            }
        }
        let shape = [rows, width];
        let mut inputs = vec![
            (
                "input_ids",
                Tensor::from_array((shape, ids)).map_err(ort_err)?.into_dyn(),
            ),
            (
                "attention_mask",
                Tensor::from_array((shape, mask)).map_err(ort_err)?.into_dyn(),
            ),
        ];
        if self.wants_types {
            inputs.push((
                "token_type_ids",
                Tensor::from_array((shape, vec![0i64; rows * width])).map_err(ort_err)?.into_dyn(),
            ));
        }
        let outputs = self.session.run(inputs).map_err(ort_err)?;
        let (out_shape, data) = outputs[self.output].try_extract_tensor::<f32>().map_err(ort_err)?;
        let dims: Vec<i64> = out_shape.iter().copied().collect();
        let (stride, hidden) = match dims.as_slice() {
            [_, seq, hidden] => ((*seq as usize) * (*hidden as usize), *hidden as usize),
            [_, hidden] => (*hidden as usize, *hidden as usize),
            other => return Err(format!("dense: the model output has shape {other:?}, expected [batch, tokens, {DIM}]")),
        };
        if hidden != DIM {
            return Err(format!("dense: the model gives {hidden} dimensions, expected {DIM}"));
        }
        let mut out = Vec::with_capacity(rows);
        for r in 0..rows {
            let start = r * stride;
            let mut v = data[start..start + hidden].to_vec();
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in v.iter_mut() {
                    *x /= norm;
                }
            }
            out.push(v);
        }
        Ok(out)
    }
}
