//! conforms: spu-classify-answer-is-the-head
//!
//! The modernbert family, the first classify family, per `weaver-spu-Spec`
//! section 11: it serves classify and nothing else, renders no template, and
//! reads its head from the artifact's own declaration, the label names being
//! the artifact's `id2label` in index order. The scores are the head's
//! softmax, finite by construction over finite logits, and a forward
//! producing otherwise faults rather than answers.

/// The architecture marker the artifact's `config.json` carries, which is
/// what selects this family among those declaring classify.
pub const ARCHITECTURE: &str = "ModernBertForSequenceClassification";

/// What a classify family declares about itself, at compile time: the
/// registry's classify half, per `weaver-spu-Spec` section 11's rule that
/// selection reads the operations declaration, decode candidates and
/// classify candidates never shadowing each other.
#[derive(Debug, Clone, Copy)]
pub struct ClassifyDeclaration {
    /// The family's name.
    pub family: &'static str,
    /// The architecture string that selects it.
    pub architecture: &'static str,
}

/// The one classify entry today. A second family joins this table the way
/// decode families join theirs, and nothing here reserves its shape.
pub const CLASSIFY_FAMILIES: &[ClassifyDeclaration] = &[ClassifyDeclaration {
    family: "modernbert",
    architecture: ARCHITECTURE,
}];

/// Select the classify family an artifact's architectures name, or nothing
/// where none declares classify for them: the operation filter is the table,
/// per `weaver-spu-Spec` section 11, so a decode admission never sees these
/// entries and this lookup never sees the decoders'.
pub fn select_classify(architectures: &[String]) -> Option<&'static ClassifyDeclaration> {
    CLASSIFY_FAMILIES
        .iter()
        .find(|declaration| architectures.iter().any(|a| a == declaration.architecture))
}

/// conforms: spu-classify-stateless
///
/// The engine half, behind the device gate the way every candle path is:
/// the forward and its plumbing. The classifier holds the admitted
/// artifact and no session type: each exchange tokenizes, forwards, and
/// answers from its own stack, nothing written that outlives the answer,
/// per `weaver-spu-Spec` section 11.
#[cfg(feature = "cuda")]
pub mod engine {
    use std::path::Path;

    use candle_core::{DType, Device, Tensor};
    use candle_nn::VarBuilder;
    use candle_transformers::models::modernbert as fork;

    /// Why an admission or an exchange could not serve. The account travels
    /// to the harness as the refusal's reason or the fault's account.
    #[derive(Debug)]
    pub enum ClassifyFault {
        NotAdmitted(String),
        Oversized {
            requested: u64,
            bound: u64,
        },
        /// The content the tokenizer refused: the ask's defect, answered as
        /// the malformed refusal, never as a device fault.
        Malformed(String),
        Forward(String),
    }

    /// The admitted classifier: the model, its tokenizer, its head's labels
    /// in index order, and the bound the artifact resolved.
    pub struct Classifier {
        model: fork::ModernBertForSequenceClassification,
        tokenizer: tokenizers::Tokenizer,
        labels: Vec<String>,
        bound: usize,
        device: Device,
    }

    impl Classifier {
        /// Admit the artifact from its directory onto the device: config,
        /// tokenizer, and weights, all read from beside the weights the way
        /// the native decode families read their sidecars. The head's labels
        /// come from `id2label` in index order, which is the artifact
        /// defining its head.
        pub fn admit(dir: &Path, device: Device) -> Result<Classifier, ClassifyFault> {
            let text = std::fs::read_to_string(dir.join("config.json"))
                .map_err(|e| ClassifyFault::NotAdmitted(format!("config unreadable: {e}")))?;
            let mut config: fork::Config = serde_json::from_str(&text)
                .map_err(|e| ClassifyFault::NotAdmitted(format!("config unparsable: {e}")))?;
            if config.classifier_config.is_none() {
                // The stock export spells label2id with integer values and
                // the fork's flattened head parse wants strings, refusing
                // silently to None, so the head is rebuilt here from the
                // parts the artifact does spell: id2label, whose values are
                // strings everywhere, and the pooling. The inversion of
                // id2label stands in for label2id, one fact spelled once.
                let raw: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| ClassifyFault::NotAdmitted(format!("config unparsable: {e}")))?;
                let id2label: std::collections::HashMap<String, String> =
                    serde_json::from_value(raw.get("id2label").cloned().unwrap_or_default())
                        .map_err(|_| {
                            ClassifyFault::NotAdmitted("no classifier head declared".into())
                        })?;
                let pooling = match raw.get("classifier_pooling").and_then(|p| p.as_str()) {
                    Some("mean") => fork::ClassifierPooling::MEAN,
                    _ => fork::ClassifierPooling::CLS,
                };
                config.classifier_config = Some(fork::ClassifierConfig {
                    label2id: id2label
                        .iter()
                        .map(|(index, label)| (label.clone(), index.clone()))
                        .collect(),
                    id2label,
                    classifier_pooling: pooling,
                });
            }
            let head = config
                .classifier_config
                .as_ref()
                .ok_or_else(|| ClassifyFault::NotAdmitted("no classifier head declared".into()))?;
            let mut labels: Vec<(usize, String)> = head
                .id2label
                .iter()
                .map(|(index, label)| {
                    index
                        .parse::<usize>()
                        .map(|i| (i, label.clone()))
                        .map_err(|_| {
                            ClassifyFault::NotAdmitted(format!(
                                "id2label key not an index: {index}"
                            ))
                        })
                })
                .collect::<Result<_, _>>()?;
            labels.sort_by_key(|(index, _)| *index);
            let labels: Vec<String> = labels.into_iter().map(|(_, label)| label).collect();
            if labels.is_empty() {
                return Err(ClassifyFault::NotAdmitted(
                    "the head declares no label".into(),
                ));
            }
            let tokenizer = tokenizers::Tokenizer::from_file(dir.join("tokenizer.json"))
                .map_err(|e| ClassifyFault::NotAdmitted(format!("tokenizer unreadable: {e}")))?;
            let weights = dir.join("model.safetensors");
            // SAFETY: the mmap is read-only over an artifact admission owns
            // for the process's life, the same posture the native decode
            // loader takes.
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights], DType::F32, &device)
                    .map_err(|e| ClassifyFault::NotAdmitted(format!("weights unloadable: {e}")))?
            };
            let model = fork::ModernBertForSequenceClassification::load(vb, &config)
                .map_err(|e| ClassifyFault::NotAdmitted(format!("model unloadable: {e}")))?;
            Ok(Classifier {
                model,
                tokenizer,
                labels,
                bound: config.max_position_embeddings,
                device,
            })
        }

        /// The head's labels in index order, the answer's denominator.
        pub fn labels(&self) -> &[String] {
            &self.labels
        }

        /// One exchange: tokenize, judge the bound in the artifact's own
        /// tokens, forward, and answer the head's softmax, every label
        /// scored. Nothing is retained.
        pub fn classify(&self, content: &str) -> Result<Vec<(String, f64)>, ClassifyFault> {
            let encoding = self
                .tokenizer
                .encode(content, true)
                .map_err(|e| ClassifyFault::Malformed(format!("tokenize refused: {e}")))?;
            let ids: Vec<u32> = encoding.get_ids().to_vec();
            if ids.len() > self.bound {
                return Err(ClassifyFault::Oversized {
                    requested: ids.len() as u64,
                    bound: self.bound as u64,
                });
            }
            let input = Tensor::new(ids.as_slice(), &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| ClassifyFault::Forward(e.to_string()))?;
            let mask = Tensor::ones(
                (1, encoding.get_ids().len()),
                candle_core::DType::U32,
                &self.device,
            )
            .map_err(|e| ClassifyFault::Forward(e.to_string()))?;
            let logits = self
                .model
                .forward(&input, &mask)
                .map_err(|e| ClassifyFault::Forward(e.to_string()))?;
            let scores = candle_nn::ops::softmax(&logits, candle_core::D::Minus1)
                .and_then(|s| s.squeeze(0))
                .and_then(|s| s.to_dtype(DType::F64))
                .and_then(|s| s.to_vec1::<f64>())
                .map_err(|e| ClassifyFault::Forward(e.to_string()))?;
            if scores.len() != self.labels.len() {
                return Err(ClassifyFault::Forward(format!(
                    "the head answered {} scores for {} labels",
                    scores.len(),
                    self.labels.len()
                )));
            }
            if scores.iter().any(|s| !s.is_finite()) {
                return Err(ClassifyFault::Forward("a score is not finite".into()));
            }
            Ok(self.labels.iter().cloned().zip(scores).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The selection is the operation filter: the classify table answers for
    /// its architecture and for no decode architecture, per the Spec's rule
    /// that neither operation's candidates shadow the other's.
    #[test]
    fn the_classify_selection_reads_the_operation_table() {
        let selected = select_classify(&[ARCHITECTURE.to_string()]).expect("selects");
        assert_eq!(selected.family, "modernbert");
        assert!(select_classify(&["Qwen2ForCausalLM".to_string()]).is_none());
        assert!(select_classify(&[]).is_none());
    }
}
