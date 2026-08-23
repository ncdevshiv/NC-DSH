use crate::{DomExceptionConstant, DomExceptionDescriptor};

pub const DOM_EXCEPTION_CONSTANTS: &[DomExceptionConstant] = &[
    DomExceptionConstant {
        property: "INDEX_SIZE_ERR",
        value: 1,
    },
    DomExceptionConstant {
        property: "DOMSTRING_SIZE_ERR",
        value: 2,
    },
    DomExceptionConstant {
        property: "HIERARCHY_REQUEST_ERR",
        value: 3,
    },
    DomExceptionConstant {
        property: "WRONG_DOCUMENT_ERR",
        value: 4,
    },
    DomExceptionConstant {
        property: "INVALID_CHARACTER_ERR",
        value: 5,
    },
    DomExceptionConstant {
        property: "NO_DATA_ALLOWED_ERR",
        value: 6,
    },
    DomExceptionConstant {
        property: "NO_MODIFICATION_ALLOWED_ERR",
        value: 7,
    },
    DomExceptionConstant {
        property: "NOT_FOUND_ERR",
        value: 8,
    },
    DomExceptionConstant {
        property: "NOT_SUPPORTED_ERR",
        value: 9,
    },
    DomExceptionConstant {
        property: "INUSE_ATTRIBUTE_ERR",
        value: 10,
    },
    DomExceptionConstant {
        property: "INVALID_STATE_ERR",
        value: 11,
    },
    DomExceptionConstant {
        property: "SYNTAX_ERR",
        value: 12,
    },
    DomExceptionConstant {
        property: "INVALID_MODIFICATION_ERR",
        value: 13,
    },
    DomExceptionConstant {
        property: "NAMESPACE_ERR",
        value: 14,
    },
    DomExceptionConstant {
        property: "INVALID_ACCESS_ERR",
        value: 15,
    },
    DomExceptionConstant {
        property: "VALIDATION_ERR",
        value: 16,
    },
    DomExceptionConstant {
        property: "TYPE_MISMATCH_ERR",
        value: 17,
    },
    DomExceptionConstant {
        property: "SECURITY_ERR",
        value: 18,
    },
    DomExceptionConstant {
        property: "NETWORK_ERR",
        value: 19,
    },
    DomExceptionConstant {
        property: "ABORT_ERR",
        value: 20,
    },
    DomExceptionConstant {
        property: "URL_MISMATCH_ERR",
        value: 21,
    },
    DomExceptionConstant {
        property: "QUOTA_EXCEEDED_ERR",
        value: 22,
    },
    DomExceptionConstant {
        property: "TIMEOUT_ERR",
        value: 23,
    },
    DomExceptionConstant {
        property: "INVALID_NODE_TYPE_ERR",
        value: 24,
    },
    DomExceptionConstant {
        property: "DATA_CLONE_ERR",
        value: 25,
    },
];

pub const DOM_EXCEPTION_DESCRIPTORS: &[DomExceptionDescriptor] = &[
    DomExceptionDescriptor {
        name: "IndexSizeError",
        legacy_code: 1,
        default_message: "Index or size was negative, or greater than the allowed value.",
    },
    DomExceptionDescriptor {
        name: "HierarchyRequestError",
        legacy_code: 3,
        default_message: "A Node was inserted somewhere it doesn't belong.",
    },
    DomExceptionDescriptor {
        name: "WrongDocumentError",
        legacy_code: 4,
        default_message: "A Node was used in a different document than the one that created it.",
    },
    DomExceptionDescriptor {
        name: "InvalidCharacterError",
        legacy_code: 5,
        default_message: "The string contains invalid characters.",
    },
    DomExceptionDescriptor {
        name: "NoModificationAllowedError",
        legacy_code: 7,
        default_message: "An attempt was made to modify an object where modifications are not allowed.",
    },
    DomExceptionDescriptor {
        name: "NotFoundError",
        legacy_code: 8,
        default_message: "An attempt was made to reference a Node in a context where it does not exist.",
    },
    DomExceptionDescriptor {
        name: "NotSupportedError",
        legacy_code: 9,
        default_message: "The implementation did not support the requested type of object or operation.",
    },
    DomExceptionDescriptor {
        name: "InUseAttributeError",
        legacy_code: 10,
        default_message: "An attempt was made to add an attribute that is already in use elsewhere.",
    },
    DomExceptionDescriptor {
        name: "InvalidStateError",
        legacy_code: 11,
        default_message: "An attempt was made to use an object that is not, or is no longer, usable.",
    },
    DomExceptionDescriptor {
        name: "SyntaxError",
        legacy_code: 12,
        default_message: "An invalid or illegal string was specified.",
    },
    DomExceptionDescriptor {
        name: "InvalidModificationError",
        legacy_code: 13,
        default_message: "The object can not be modified in this way.",
    },
    DomExceptionDescriptor {
        name: "NamespaceError",
        legacy_code: 14,
        default_message: "An attempt was made to create or change an object in a way which is incorrect with regard to namespaces.",
    },
    DomExceptionDescriptor {
        name: "InvalidAccessError",
        legacy_code: 15,
        default_message: "A parameter or an operation was not supported by the underlying object.",
    },
    DomExceptionDescriptor {
        name: "TypeMismatchError",
        legacy_code: 17,
        default_message: "The type of an object was incompatible with the expected type of the parameter associated to the object.",
    },
    DomExceptionDescriptor {
        name: "SecurityError",
        legacy_code: 18,
        default_message: "An attempt was made to break through the security policy of the user agent.",
    },
    DomExceptionDescriptor {
        name: "NetworkError",
        legacy_code: 19,
        default_message: "A network error occurred.",
    },
    DomExceptionDescriptor {
        name: "AbortError",
        legacy_code: 20,
        default_message: "The user aborted a request.",
    },
    DomExceptionDescriptor {
        name: "URLMismatchError",
        legacy_code: 21,
        default_message: "A worker global scope represented an absolute URL that is not equal to the resulting absolute URL.",
    },
    DomExceptionDescriptor {
        name: "QuotaExceededError",
        legacy_code: 22,
        default_message: "An attempt was made to add something to storage that exceeded the quota.",
    },
    DomExceptionDescriptor {
        name: "TimeoutError",
        legacy_code: 23,
        default_message: "A timeout occurred.",
    },
    DomExceptionDescriptor {
        name: "InvalidNodeTypeError",
        legacy_code: 24,
        default_message: "The supplied node is invalid or has an invalid ancestor for this operation.",
    },
    DomExceptionDescriptor {
        name: "DataCloneError",
        legacy_code: 25,
        default_message: "An object could not be cloned.",
    },
    DomExceptionDescriptor {
        name: "EncodingError",
        legacy_code: 0,
        default_message: "A URI supplied to the API was malformed, or the resulting Data URL has exceeded the URL length limitations for Data URLs.",
    },
    DomExceptionDescriptor {
        name: "NotReadableError",
        legacy_code: 0,
        default_message: "The requested file could not be read, typically due to permission problems that have occurred after a reference to a file was acquired.",
    },
    DomExceptionDescriptor {
        name: "UnknownError",
        legacy_code: 0,
        default_message: "The operation failed for an unknown transient reason.",
    },
    DomExceptionDescriptor {
        name: "ConstraintError",
        legacy_code: 0,
        default_message: "A mutation operation in the transaction failed because a constraint was not satisfied.",
    },
    DomExceptionDescriptor {
        name: "DataError",
        legacy_code: 0,
        default_message: "The data provided does not meet requirements.",
    },
    DomExceptionDescriptor {
        name: "TransactionInactiveError",
        legacy_code: 0,
        default_message: "A request was placed against a transaction which is either currently not active, or which is finished.",
    },
    DomExceptionDescriptor {
        name: "ReadOnlyError",
        legacy_code: 0,
        default_message: "A write operation was attempted in a read-only transaction.",
    },
    DomExceptionDescriptor {
        name: "VersionError",
        legacy_code: 0,
        default_message: "An attempt was made to open a database using a lower version than the existing version.",
    },
    DomExceptionDescriptor {
        name: "OperationError",
        legacy_code: 0,
        default_message: "The operation failed for an operation-specific reason.",
    },
    DomExceptionDescriptor {
        name: "NotAllowedError",
        legacy_code: 0,
        default_message: "The request is not allowed by the user agent or the platform in the current context.",
    },
    DomExceptionDescriptor {
        name: "OptOutError",
        legacy_code: 0,
        default_message: "The user opted out of the process.",
    },
    DomExceptionDescriptor {
        name: "PathExistsError",
        legacy_code: 0,
        default_message: "An attempt was made to create a file or directory where an element already exists.",
    },
    DomExceptionDescriptor {
        name: "PermissionDeniedError",
        legacy_code: 0,
        default_message: "User or security policy denied the request.",
    },
    DomExceptionDescriptor {
        name: "BreakError",
        legacy_code: 0,
        default_message: "A break condition has been detected.",
    },
    DomExceptionDescriptor {
        name: "BufferOverrunError",
        legacy_code: 0,
        default_message: "A buffer overrun has been detected.",
    },
    DomExceptionDescriptor {
        name: "FramingError",
        legacy_code: 0,
        default_message: "A framing error has been detected.",
    },
    DomExceptionDescriptor {
        name: "ParityError",
        legacy_code: 0,
        default_message: "A parity error has been detected.",
    },
    DomExceptionDescriptor {
        name: "WebTransportError",
        legacy_code: 0,
        default_message: "The WebTransport operation failed.",
    },
    DomExceptionDescriptor {
        name: "SmartCardError",
        legacy_code: 0,
        default_message: "A Smart Card operation failed.",
    },
    DomExceptionDescriptor {
        name: "GPUPipelineError",
        legacy_code: 0,
        default_message: "A WebGPU pipeline creation failed.",
    },
    DomExceptionDescriptor {
        name: "OverconstrainedError",
        legacy_code: 0,
        default_message: "The desired set of constraints/capabilities cannot be met.",
    },
    DomExceptionDescriptor {
        name: "IdentityCredentialError",
        legacy_code: 0,
        default_message: "An attempt to retrieve an IdentityCredential has failed.",
    },
    DomExceptionDescriptor {
        name: "WebSocketError",
        legacy_code: 0,
        default_message: "The WebSocket connection was closed.",
    },
];
