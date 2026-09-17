use super::*;

pub(crate) fn resource_api_error(status: &kube::core::Status) -> ResourceApiError {
    ResourceApiError {
        status_code: status.code,
        message: status.message.clone(),
        causes: status
            .details
            .as_ref()
            .map_or(&[][..], |details| details.causes.as_slice())
            .iter()
            .map(|cause| ResourceApiErrorCause {
                field: cause.field.clone(),
                message: cause.message.clone(),
                reason: cause.reason.clone(),
            })
            .collect(),
    }
}

pub(crate) fn resource_version_conflict_error() -> ResourceApiError {
    ResourceApiError {
        status_code: 409,
        message: "The resource changed on the cluster".into(),
        causes: Vec::new(),
    }
}
