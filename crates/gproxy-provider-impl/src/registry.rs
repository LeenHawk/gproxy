use std::collections::HashMap;
use std::sync::Arc;

use gproxy_provider_core::{NoopStateSink, PoolSnapshot, Provider, StateSink};

use crate::credential::BaseCredential;
use crate::provider::{
    AistudioProvider, AntiGravityProvider, ClaudeCodeProvider, ClaudeProvider, CodexProvider,
    DeepSeekProvider, GeminiCliProvider, NvidiaProvider, OpenAIProvider, VertexExpressProvider,
    VertexProvider,
};

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn Provider>>,
    openai: Arc<OpenAIProvider>,
    claude: Arc<ClaudeProvider>,
    aistudio: Arc<AistudioProvider>,
    vertexexpress: Arc<VertexExpressProvider>,
    vertex: Arc<VertexProvider>,
    geminicli: Arc<GeminiCliProvider>,
    claudecode: Arc<ClaudeCodeProvider>,
    codex: Arc<CodexProvider>,
    antigravity: Arc<AntiGravityProvider>,
    nvidia: Arc<NvidiaProvider>,
    deepseek: Arc<DeepSeekProvider>,
}

impl ProviderRegistry {
    pub fn get(&self, name: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(name).cloned()
    }

    pub fn openai(&self) -> Arc<OpenAIProvider> {
        self.openai.clone()
    }

    pub fn claude(&self) -> Arc<ClaudeProvider> {
        self.claude.clone()
    }

    pub fn aistudio(&self) -> Arc<AistudioProvider> {
        self.aistudio.clone()
    }

    pub fn vertexexpress(&self) -> Arc<VertexExpressProvider> {
        self.vertexexpress.clone()
    }

    pub fn vertex(&self) -> Arc<VertexProvider> {
        self.vertex.clone()
    }

    pub fn geminicli(&self) -> Arc<GeminiCliProvider> {
        self.geminicli.clone()
    }

    pub fn claudecode(&self) -> Arc<ClaudeCodeProvider> {
        self.claudecode.clone()
    }

    pub fn codex(&self) -> Arc<CodexProvider> {
        self.codex.clone()
    }

    pub fn antigravity(&self) -> Arc<AntiGravityProvider> {
        self.antigravity.clone()
    }

    pub fn nvidia(&self) -> Arc<NvidiaProvider> {
        self.nvidia.clone()
    }

    pub fn deepseek(&self) -> Arc<DeepSeekProvider> {
        self.deepseek.clone()
    }

    pub fn apply_pools(&self, mut pools: HashMap<String, PoolSnapshot<BaseCredential>>) {
        if let Some(pool) = pools.remove("openai") {
            self.openai.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("claude") {
            self.claude.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("aistudio") {
            self.aistudio.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("vertexexpress") {
            self.vertexexpress.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("vertex") {
            self.vertex.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("geminicli") {
            self.geminicli.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("claudecode") {
            self.claudecode.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("codex") {
            self.codex.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("antigravity") {
            self.antigravity.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("nvidia") {
            self.nvidia.replace_snapshot(pool);
        }
        if let Some(pool) = pools.remove("deepseek") {
            self.deepseek.replace_snapshot(pool);
        }
    }
}

pub fn build_registry() -> ProviderRegistry {
    build_registry_with_sink(Arc::new(NoopStateSink))
}

pub fn build_registry_with_sink(sink: Arc<dyn StateSink>) -> ProviderRegistry {
    let openai = Arc::new(OpenAIProvider::new(sink.clone()));
    let claude = Arc::new(ClaudeProvider::new(sink.clone()));
    let aistudio = Arc::new(AistudioProvider::new(sink.clone()));
    let vertexexpress = Arc::new(VertexExpressProvider::new(sink.clone()));
    let vertex = Arc::new(VertexProvider::new(sink.clone()));
    let geminicli = Arc::new(GeminiCliProvider::new(sink.clone()));
    let claudecode = Arc::new(ClaudeCodeProvider::new(sink.clone()));
    let codex = Arc::new(CodexProvider::new(sink.clone()));
    let antigravity = Arc::new(AntiGravityProvider::new(sink.clone()));
    let nvidia = Arc::new(NvidiaProvider::new(sink.clone()));
    let deepseek = Arc::new(DeepSeekProvider::new(sink));

    let mut providers: HashMap<String, Arc<dyn Provider>> = HashMap::new();
    providers.insert(openai.name().to_string(), openai.clone());
    providers.insert(claude.name().to_string(), claude.clone());
    providers.insert(aistudio.name().to_string(), aistudio.clone());
    providers.insert(vertexexpress.name().to_string(), vertexexpress.clone());
    providers.insert(vertex.name().to_string(), vertex.clone());
    providers.insert(geminicli.name().to_string(), geminicli.clone());
    providers.insert(claudecode.name().to_string(), claudecode.clone());
    providers.insert(codex.name().to_string(), codex.clone());
    providers.insert(antigravity.name().to_string(), antigravity.clone());
    providers.insert(nvidia.name().to_string(), nvidia.clone());
    providers.insert(deepseek.name().to_string(), deepseek.clone());

    ProviderRegistry {
        providers,
        openai,
        claude,
        aistudio,
        vertexexpress,
        vertex,
        geminicli,
        claudecode,
        codex,
        antigravity,
        nvidia,
        deepseek,
    }
}
