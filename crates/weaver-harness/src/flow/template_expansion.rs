//! Runtime bytes from the actual recorded template certificate.
//!
//! This is the literal-only restatement family. Referenced rejected text stays
//! in instruction metadata and never becomes a substitution. Step-reference
//! spelling and the other instruction families remain with their own ports.

use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::state_flow::{Digest, SchemeDeclaration, TemplatePart};

use super::{TemplateAcceptance, TemplateCertificate};

/// Expected identities come from the caller's recorded load, not a template.
pub(in crate::flow) struct ExpansionIdentity<'a> {
    pub run_id: &'a str,
    pub scheme: &'a SchemeDeclaration,
    pub checker_artifact_sha256: &'a Digest,
    pub inventory_sha256: &'a Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::flow) enum ExpansionRefusal {
    InvalidTemplate,
    ForeignCertificate,
    InvalidCertificate,
    UnsupportedSlot,
    EmptyBody,
}

/// The compiled consumer elects this family. No runtime string selects it.
#[derive(Debug)]
pub(in crate::flow) struct RestatementTemplate {
    template_id: &'static str,
}

impl RestatementTemplate {
    pub(in crate::flow) const fn new(template_id: &'static str) -> Self {
        Self { template_id }
    }

    pub(in crate::flow) fn template_id(&self) -> &'static str {
        self.template_id
    }

    /// An expansion proves certified bytes, not that an instruction was
    /// recorded, that its source is rejected, or that a decoder may run.
    pub(in crate::flow) fn expand(
        &'static self,
        certificate: &TemplateCertificate,
        identity: ExpansionIdentity<'_>,
    ) -> Result<TemplateExpansion, ExpansionRefusal> {
        if identity.run_id.is_empty()
            || identity.scheme.validate().is_err()
            || certificate.run_id() != identity.run_id
            || certificate.scheme() != identity.scheme
            || certificate.scheme_id() != identity.scheme.scheme_id
            || certificate.checker_artifact_sha256() != identity.checker_artifact_sha256
            || certificate.inventory_sha256() != identity.inventory_sha256
        {
            return Err(ExpansionRefusal::ForeignCertificate);
        }
        let inventory = certificate.inventory();
        if certificate.inventory_sha256() != inventory.digest()
            || certificate.accepted().len() != inventory.templates().len()
        {
            return Err(ExpansionRefusal::InvalidCertificate);
        }
        let Some((index, template)) = inventory
            .templates()
            .iter()
            .enumerate()
            .find(|(_, template)| template.template_id == self.template_id)
        else {
            return Err(ExpansionRefusal::InvalidTemplate);
        };
        if self.template_id.is_empty() || template.kind != "loop.instruction" {
            return Err(ExpansionRefusal::InvalidTemplate);
        }
        let acceptance = &certificate.accepted()[index];
        if acceptance.template_id() != self.template_id
            || acceptance.request_id().0 == 0
            || acceptance.result_source().block_id.run != identity.run_id
            || acceptance.result_source().block_id.sequence.0 == 0
            || acceptance.result_source().block_id.block.0 != 0
            || acceptance.result_source().turn.is_some()
        {
            return Err(ExpansionRefusal::InvalidCertificate);
        }
        let mut text = String::new();
        for part in &template.parts {
            match part {
                TemplatePart::Literal(literal) => text.push_str(literal),
                TemplatePart::Slot { .. } => return Err(ExpansionRefusal::UnsupportedSlot),
            }
        }
        if text.is_empty() {
            return Err(ExpansionRefusal::EmptyBody);
        }
        Ok(TemplateExpansion {
            template_id: self.template_id,
            message: Message {
                role: Role::User,
                content: vec![ContentBlock::Text { text }],
            },
            run_id: certificate.run_id().into(),
            scheme: certificate.scheme().clone(),
            checker_artifact_sha256: certificate.checker_artifact_sha256().clone(),
            inventory_sha256: certificate.inventory_sha256().clone(),
            acceptance: TemplateAcceptance {
                template_id: acceptance.template_id().into(),
                request_id: acceptance.request_id(),
                result_source: acceptance.result_source().clone(),
            },
        })
    }
}

/// Private, owned evidence of this expansion's grammar certificate. The
/// recording owner must still prove metadata and record the instruction once.
#[derive(Debug)]
pub(in crate::flow) struct TemplateExpansion {
    template_id: &'static str,
    message: Message,
    run_id: String,
    scheme: SchemeDeclaration,
    checker_artifact_sha256: Digest,
    inventory_sha256: Digest,
    acceptance: TemplateAcceptance,
}

impl TemplateExpansion {
    pub(in crate::flow) fn template_id(&self) -> &str {
        self.template_id
    }
    pub(in crate::flow) fn message(&self) -> &Message {
        &self.message
    }
    pub(in crate::flow) fn text(&self) -> &str {
        let ContentBlock::Text { text } = &self.message.content[0] else {
            unreachable!("a certified expansion contains one Text block")
        };
        text
    }
    pub(in crate::flow) fn run_id(&self) -> &str {
        &self.run_id
    }
    pub(in crate::flow) fn scheme(&self) -> &SchemeDeclaration {
        &self.scheme
    }
    pub(in crate::flow) fn checker_artifact_sha256(&self) -> &Digest {
        &self.checker_artifact_sha256
    }
    pub(in crate::flow) fn inventory_sha256(&self) -> &Digest {
        &self.inventory_sha256
    }
    pub(in crate::flow) fn acceptance(&self) -> &TemplateAcceptance {
        &self.acceptance
    }
}

#[cfg(test)]
#[path = "template_expansion_tests.rs"]
mod tests;
