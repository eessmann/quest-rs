//! One bounded paired-history campaign row; the external runner enforces OS/time caps.
use quest_cfd::paired_history::{
	PairedHistoryLimits, PairedHistoryRequest, run_paired_history_row,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut args = std::env::args().skip(1);
	if args.next().as_deref() != Some("--request-json") {
		return Err("expected --request-json JSON".into());
	}
	let text = args.next().ok_or("missing request")?;
	if text.len() > 4096 || args.next().is_some() {
		return Err("unexpected or oversized request".into());
	}
	let request: PairedHistoryRequest = serde_json::from_str(&text)?;
	let limits = PairedHistoryLimits::default();
	let result = match run_paired_history_row(request, limits) {
		Ok(row) => json!({"status":"completed","row":row}),
		Err(error) => {
			json!({"status":"rejected","request":request,"limits":limits,"error":error.to_string()})
		}
	};
	println!("{}", serde_json::to_string(&result)?);
	Ok(())
}
