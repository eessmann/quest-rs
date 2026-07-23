use quest_sys::{QuestEnvironment, QuestResult};
use std::sync::OnceLock;

static ENVIRONMENT_CLAIMED: OnceLock<()> = OnceLock::new();

fn claim_environment() -> quest_sys::QuestResult<()> {
    ENVIRONMENT_CLAIMED.set(()).map_err(|()| {
        quest_sys::QuestError::Lifecycle(
            "the QuEST environment has already been initialized or finalized; \
             restarting it is unsupported"
                .into(),
        )
    })
}

pub struct QuESTEnvironment {
    env: QuestEnvironment,
}

impl QuESTEnvironment {
    pub fn new() -> QuestResult<Self> {
        quest_sys::init_quest_env()?;
        Ok(Self {
            env: quest_sys::get_quest_env()?,
        })
    }

    pub fn with_custom_config(
        use_distribution: bool,
        use_gpu: bool,
        use_multithreading: bool,
    ) -> QuestResult<Self> {
        quest_sys::init_custom_quest_env(use_distribution, use_gpu, use_multithreading)?;
        Ok(Self {
            env: quest_sys::get_quest_env()?,
        })
    }

    pub fn env(&self) -> &QuestEnvironment {
        &self.env
    }
}

impl Drop for QuESTEnvironment {
    fn drop(&mut self) {
        let _ = quest_sys::finalize_quest_env();
    }
}
