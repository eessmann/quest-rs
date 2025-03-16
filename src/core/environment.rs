use quest_sys::QuESTEnv;
use cxx::UniquePtr;
pub struct QuESTEnvironment {
    env: UniquePtr<QuESTEnv>,
}

impl QuESTEnvironment {
    pub fn new() -> Self {
        quest_sys::initQuESTEnv();
        Self { env: quest_sys::getQuESTEnv() }
    }

    pub fn with_custom_config(use_distribution: bool, use_gpu: bool, use_multithreading: bool) -> Self {
        quest_sys::initCustomQuESTEnv(use_distribution, use_gpu, use_multithreading);
        Self { env: quest_sys::getQuESTEnv() }
    }
}

impl Drop for QuESTEnvironment {
    fn drop(&mut self) {
        quest_sys::finalizeQuESTEnv();
    }
}