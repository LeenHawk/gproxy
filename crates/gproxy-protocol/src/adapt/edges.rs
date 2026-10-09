//! The conversion edges this crate adapts.
//!
//! An edge reads: a client request in the source key is served by an
//! upstream call in the target key, response mapping included. The return
//! path is part of the edge, never a reverse edge of its own. There is no
//! transitive closure: an edge exists only when it is listed here, and the
//! host must not offer a conversion that is not. `design/transform-matrix.md`
//! records why each family has the shape it has.
//!
//! Dialect letters follow the matrix: R = Responses, H = Chat Completions,
//! C = Claude, G = Gemini, W = Responses over a websocket.

use crate::{Dialect, Operation, OperationKey};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConversionEdge {
    pub source: OperationKey,
    pub target: OperationKey,
}

const fn edge(
    (source_operation, source_dialect): (Operation, Dialect),
    (target_operation, target_dialect): (Operation, Dialect),
) -> ConversionEdge {
    ConversionEdge {
        source: OperationKey {
            operation: source_operation,
            dialect: source_dialect,
        },
        target: OperationKey {
            operation: target_operation,
            dialect: target_dialect,
        },
    }
}

pub const CONVERSION_EDGES: &[ConversionEdge] = {
    use Dialect::{
        Claude as C, Gemini as G, OpenAi as R, OpenAiChat as H, OpenAiResponsesWebSocket as W,
    };
    use Operation::{GenerateContent as Generate, StreamGenerateContent as Stream, *};
    &[
        // ListModels
        edge((ListModels, R), (ListModels, C)),
        edge((ListModels, R), (ListModels, G)),
        edge((ListModels, C), (ListModels, R)),
        edge((ListModels, C), (ListModels, G)),
        edge((ListModels, G), (ListModels, R)),
        edge((ListModels, G), (ListModels, C)),
        // GetModel
        edge((GetModel, R), (GetModel, C)),
        edge((GetModel, R), (GetModel, G)),
        edge((GetModel, C), (GetModel, R)),
        edge((GetModel, C), (GetModel, G)),
        edge((GetModel, G), (GetModel, R)),
        edge((GetModel, G), (GetModel, C)),
        // CountTokens
        edge((CountTokens, R), (CountTokens, C)),
        edge((CountTokens, R), (CountTokens, G)),
        edge((CountTokens, C), (CountTokens, G)),
        edge((CountTokens, G), (CountTokens, C)),
        // GenerateContent
        edge((Generate, R), (Generate, C)),
        edge((Generate, R), (Generate, G)),
        edge((Generate, R), (Generate, H)),
        edge((Generate, R), (Stream, R)),
        edge((Generate, R), (Stream, C)),
        edge((Generate, R), (Stream, G)),
        edge((Generate, R), (Stream, H)),
        edge((Generate, R), (Stream, W)),
        edge((Generate, C), (Generate, R)),
        edge((Generate, C), (Generate, G)),
        edge((Generate, C), (Generate, H)),
        edge((Generate, C), (Stream, R)),
        edge((Generate, C), (Stream, C)),
        edge((Generate, C), (Stream, G)),
        edge((Generate, C), (Stream, H)),
        edge((Generate, G), (Generate, R)),
        edge((Generate, G), (Generate, C)),
        edge((Generate, G), (Generate, H)),
        edge((Generate, G), (Stream, R)),
        edge((Generate, G), (Stream, C)),
        edge((Generate, G), (Stream, G)),
        edge((Generate, G), (Stream, H)),
        edge((Generate, H), (Generate, R)),
        edge((Generate, H), (Generate, C)),
        edge((Generate, H), (Generate, G)),
        edge((Generate, H), (Stream, R)),
        edge((Generate, H), (Stream, C)),
        edge((Generate, H), (Stream, G)),
        edge((Generate, H), (Stream, H)),
        // StreamGenerateContent
        edge((Stream, R), (Generate, R)),
        edge((Stream, R), (Generate, C)),
        edge((Stream, R), (Generate, G)),
        edge((Stream, R), (Generate, H)),
        edge((Stream, R), (Stream, C)),
        edge((Stream, R), (Stream, G)),
        edge((Stream, R), (Stream, H)),
        edge((Stream, R), (Stream, W)),
        edge((Stream, C), (Generate, R)),
        edge((Stream, C), (Generate, C)),
        edge((Stream, C), (Generate, G)),
        edge((Stream, C), (Generate, H)),
        edge((Stream, C), (Stream, R)),
        edge((Stream, C), (Stream, G)),
        edge((Stream, C), (Stream, H)),
        edge((Stream, C), (Stream, W)),
        edge((Stream, H), (Generate, R)),
        edge((Stream, H), (Generate, C)),
        edge((Stream, H), (Generate, G)),
        edge((Stream, H), (Generate, H)),
        edge((Stream, H), (Stream, R)),
        edge((Stream, H), (Stream, C)),
        edge((Stream, H), (Stream, G)),
        edge((Stream, H), (Stream, W)),
        edge((Stream, G), (Generate, R)),
        edge((Stream, G), (Generate, C)),
        edge((Stream, G), (Generate, G)),
        edge((Stream, G), (Generate, H)),
        edge((Stream, G), (Stream, R)),
        edge((Stream, G), (Stream, C)),
        edge((Stream, G), (Stream, H)),
        edge((Stream, G), (Stream, W)),
        // GuardianReview
        edge((GuardianReview, R), (Generate, R)),
        edge((GuardianReview, R), (Generate, C)),
        edge((GuardianReview, R), (Generate, G)),
        edge((GuardianReview, R), (Generate, H)),
        // GuardianClassify
        edge((GuardianClassify, R), (Generate, R)),
        edge((GuardianClassify, R), (Generate, C)),
        edge((GuardianClassify, R), (Generate, G)),
        edge((GuardianClassify, R), (Generate, H)),
        // CompactContent
        edge((CompactContent, R), (Generate, R)),
        edge((CompactContent, R), (Generate, C)),
        edge((CompactContent, R), (Generate, G)),
        edge((CompactContent, R), (Generate, H)),
        // SummarizeMemory
        edge((SummarizeMemory, R), (Generate, R)),
        edge((SummarizeMemory, R), (Generate, C)),
        edge((SummarizeMemory, R), (Generate, G)),
        edge((SummarizeMemory, R), (Generate, H)),
        // WebSearch
        edge((WebSearch, R), (Generate, R)),
        edge((WebSearch, R), (Generate, C)),
        edge((WebSearch, R), (Generate, G)),
        // CreateEmbedding
        edge((CreateEmbedding, R), (CreateEmbedding, G)),
        edge((CreateEmbedding, G), (CreateEmbedding, R)),
        // BatchCreateEmbedding
        edge((BatchCreateEmbedding, G), (CreateEmbedding, R)),
        // CreateImage
        edge((CreateImage, R), (Generate, G)),
        // EditImage
        edge((EditImage, R), (Generate, G)),
        // CreateFile
        edge((CreateFile, R), (CreateFile, C)),
        edge((CreateFile, R), (CreateFile, G)),
        edge((CreateFile, C), (CreateFile, R)),
        edge((CreateFile, C), (CreateFile, G)),
        edge((CreateFile, G), (CreateFile, R)),
        edge((CreateFile, G), (CreateFile, C)),
        // ListFiles
        edge((ListFiles, R), (ListFiles, C)),
        edge((ListFiles, R), (ListFiles, G)),
        edge((ListFiles, C), (ListFiles, R)),
        edge((ListFiles, C), (ListFiles, G)),
        edge((ListFiles, G), (ListFiles, R)),
        edge((ListFiles, G), (ListFiles, C)),
        // RetrieveFile
        edge((RetrieveFile, R), (RetrieveFile, C)),
        edge((RetrieveFile, R), (RetrieveFile, G)),
        edge((RetrieveFile, C), (RetrieveFile, R)),
        edge((RetrieveFile, C), (RetrieveFile, G)),
        edge((RetrieveFile, G), (RetrieveFile, R)),
        edge((RetrieveFile, G), (RetrieveFile, C)),
        // RetrieveFileContent
        edge((RetrieveFileContent, R), (RetrieveFileContent, C)),
        edge((RetrieveFileContent, R), (RetrieveFileContent, G)),
        edge((RetrieveFileContent, C), (RetrieveFileContent, R)),
        edge((RetrieveFileContent, C), (RetrieveFileContent, G)),
        edge((RetrieveFileContent, G), (RetrieveFileContent, R)),
        edge((RetrieveFileContent, G), (RetrieveFileContent, C)),
        // DeleteFile
        edge((DeleteFile, R), (DeleteFile, C)),
        edge((DeleteFile, R), (DeleteFile, G)),
        edge((DeleteFile, C), (DeleteFile, R)),
        edge((DeleteFile, C), (DeleteFile, G)),
        edge((DeleteFile, G), (DeleteFile, R)),
        edge((DeleteFile, G), (DeleteFile, C)),
        // CreateVideo
        edge((CreateVideo, R), (CreateVideo, G)),
        // RetrieveVideo
        edge((RetrieveVideo, R), (RetrieveVideo, G)),
        // DownloadVideoContent
        edge((DownloadVideoContent, R), (DownloadVideoContent, G)),
        // GenerateContent
        edge((Generate, W), (Generate, R)),
        edge((Generate, W), (Generate, C)),
        edge((Generate, W), (Generate, G)),
        edge((Generate, W), (Generate, H)),
        edge((Generate, W), (Stream, R)),
        edge((Generate, W), (Stream, C)),
        edge((Generate, W), (Stream, G)),
        edge((Generate, W), (Stream, H)),
        // StreamGenerateContent
        edge((Stream, W), (Generate, R)),
        edge((Stream, W), (Generate, C)),
        edge((Stream, W), (Generate, G)),
        edge((Stream, W), (Generate, H)),
        edge((Stream, W), (Stream, R)),
        edge((Stream, W), (Stream, C)),
        edge((Stream, W), (Stream, G)),
        edge((Stream, W), (Stream, H)),
    ]
};

pub fn is_conversion_edge(source: OperationKey, target: OperationKey) -> bool {
    CONVERSION_EDGES.contains(&ConversionEdge { source, target })
}

/// Targets that can serve `source`, in table order.
pub fn conversion_targets(source: OperationKey) -> impl Iterator<Item = OperationKey> {
    CONVERSION_EDGES
        .iter()
        .filter(move |edge| edge.source == source)
        .map(|edge| edge.target)
}
