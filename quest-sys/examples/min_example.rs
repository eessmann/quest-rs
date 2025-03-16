use quest_sys::*;

fn main() {
    initQuESTEnv();
    reportQuESTEnv();
    
    let mut qureg = createQureg(20);
    reportQuregParams(&qureg);
    
    initRandomPureState(qureg.pin_mut());
    reportQureg(&qureg);
    
    let prob = calcTotalProb(&qureg);
    println!("Total probability: {}", prob);
    
    destroyQureg(qureg.pin_mut());
    finalizeQuESTEnv()
}