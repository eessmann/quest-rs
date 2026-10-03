#![cfg(feature = "serde")]
use dashu_int::IBig;
use googletest::prelude::*;
use quest_math::AngleTarget;

#[gtest]
fn angle_interchange_reduces_signed_pairs_to_decimal_strings() -> googletest::Result<()> {
	let target = AngleTarget::RationalPi {
		numerator: IBig::from(-6),
		denominator: IBig::from(-8),
	};
	expect_eq!(
		serde_json::to_value(&target)?,
		serde_json::json!({"RationalPi":{"numerator":"3","denominator":"4"}})
	);
	let affine = AngleTarget::AffinePi {
		radians_numerator: IBig::from(0),
		radians_denominator: IBig::from(19),
		pi_numerator: IBig::from(6),
		pi_denominator: IBig::from(-8),
	};
	expect_eq!(
		serde_json::to_value(&affine)?,
		serde_json::json!({"AffinePi":{
			"radians_numerator":"0", "radians_denominator":"1", "pi_numerator":"-3", "pi_denominator":"4"
		}})
	);
	Ok(())
}

#[gtest]
fn angle_interchange_rejects_noncanonical_or_unreduced_pairs() -> googletest::Result<()> {
	for (numerator, denominator) in [
		("+2", "3"),
		("-0", "1"),
		("02", "3"),
		("2", "0"),
		("2", "-3"),
		("2", "4"),
		("0", "9"),
	] {
		let encoded =
			serde_json::json!({"RationalPi":{"numerator":numerator,"denominator":denominator}});
		expect_true!(
			serde_json::from_value::<AngleTarget>(encoded).is_err(),
			"{numerator}/{denominator}"
		);
	}
	let valid = serde_json::json!({"RationalPi":{"numerator":"-2","denominator":"3"}});
	let decoded: AngleTarget = serde_json::from_value(valid.clone())?;
	expect_eq!(serde_json::to_value(decoded)?, valid);
	Ok(())
}
