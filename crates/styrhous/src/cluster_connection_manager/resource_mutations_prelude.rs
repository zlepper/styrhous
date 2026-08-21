use super::*;

pub(crate) fn resource_api_error(status: &kube::core::Status) -> ResourceApiError {
    ResourceApiError {
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
