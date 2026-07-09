fn main() -> quest_sys::QuestResult<()> {
    quest_sys::init_quest_env()?;

    let report = quest_sys::get_environment_string()?;
    println!("{report}");

    let mut qureg = quest_sys::create_qureg(20)?;
    quest_sys::init_plus_state(qureg.pin_mut())?;

    let prob = quest_sys::calc_total_prob(&qureg)?;
    println!("Total probability: {prob}");

    drop(qureg);
    quest_sys::finalize_quest_env()
}
