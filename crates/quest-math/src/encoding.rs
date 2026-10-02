//! Canonical decimal interchange for exact mathematical values.
use dashu_int::{IBig, UBig};
use dashu_ratio::RBig;

/// Parse a canonical signed decimal integer.
/// # Errors
/// Rejects nondecimal spelling, positive signs, leading zeroes, or negative zero.
pub fn parse_integer(value: &str) -> Result<IBig, &'static str> {
    let integer: IBig = value.parse().map_err(|_| "invalid decimal integer")?;
    if integer.to_string() != value {
        return Err("noncanonical decimal integer");
    }
    Ok(integer)
}

/// Parse a reduced rational with a positive decimal denominator.
/// # Errors
/// Rejects noncanonical integers, nonpositive denominators, or unreduced pairs.
pub fn parse_rational(numerator: &str, denominator: &str) -> Result<RBig, &'static str> {
    let numerator = parse_integer(numerator)?;
    let denominator = parse_integer(denominator)?;
    let denominator = UBig::try_from(denominator).map_err(|_| "nonpositive denominator")?;
    if denominator.is_zero() {
        return Err("nonpositive denominator");
    }
    let rational = RBig::from_parts(numerator.clone(), denominator.clone());
    if rational.numerator() != &numerator || rational.denominator() != &denominator {
        return Err("unreduced rational");
    }
    Ok(rational)
}

#[cfg(feature = "serde")]
pub mod integer {
    use super::parse_integer;
    use dashu_int::IBig;
    use serde::{Deserialize, Deserializer, Serializer};
    /// Serialize a canonical decimal integer.
    /// # Errors
    /// Forwards serializer failures.
    pub fn serialize<S: Serializer>(value: &IBig, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    /// Deserialize a canonical decimal integer.
    /// # Errors
    /// Rejects noncanonical integer strings.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<IBig, D::Error> {
        parse_integer(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
pub mod integer_array {
    use super::parse_integer;
    use dashu_int::IBig;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    /// Serialize an exact coefficient array as decimal strings.
    /// # Errors
    /// Forwards serializer failures.
    pub fn serialize<S: Serializer, const N: usize>(
        value: &[IBig; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    /// Deserialize a coefficient array of canonical decimal strings.
    /// # Errors
    /// Rejects noncanonical strings or incorrect array lengths.
    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        deserializer: D,
    ) -> Result<[IBig; N], D::Error> {
        Vec::<String>::deserialize(deserializer)?
            .into_iter()
            .map(|value| parse_integer(&value).map_err(serde::de::Error::custom))
            .collect::<Result<Vec<_>, D::Error>>()?
            .try_into()
            .map_err(|_| serde::de::Error::custom("incorrect coefficient array length"))
    }
}

#[cfg(feature = "serde")]
pub mod rational {
    use super::parse_rational;
    use dashu_ratio::RBig;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    #[derive(Serialize, Deserialize)]
    struct Pair {
        numerator: String,
        denominator: String,
    }
    /// Serialize a reduced decimal numerator/denominator pair.
    /// # Errors
    /// Forwards serializer failures.
    pub fn serialize<S: Serializer>(value: &RBig, serializer: S) -> Result<S::Ok, S::Error> {
        Pair {
            numerator: value.numerator().to_string(),
            denominator: value.denominator().to_string(),
        }
        .serialize(serializer)
    }
    /// Deserialize a reduced pair with a positive denominator.
    /// # Errors
    /// Rejects noncanonical or unreduced pairs.
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<RBig, D::Error> {
        let pair = Pair::deserialize(deserializer)?;
        parse_rational(&pair.numerator, &pair.denominator).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
mod target {
    use super::parse_rational;
    use crate::AngleTarget;
    use dashu_int::IBig;
    use dashu_ratio::RBig;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    enum Wire {
        DyadicRadians {
            bits: u64,
        },
        RationalPi {
            numerator: String,
            denominator: String,
        },
        AffinePi {
            radians_numerator: String,
            radians_denominator: String,
            pi_numerator: String,
            pi_denominator: String,
        },
    }
    fn pair<E: serde::ser::Error>(
        numerator: &IBig,
        denominator: &IBig,
    ) -> Result<(String, String), E> {
        if denominator.is_zero() {
            return Err(E::custom("zero denominator"));
        }
        let reduced = RBig::from_parts_signed(numerator.clone(), denominator.clone());
        Ok((
            reduced.numerator().to_string(),
            reduced.denominator().to_string(),
        ))
    }
    fn decoded_pair<E: serde::de::Error>(
        numerator: &str,
        denominator: &str,
    ) -> Result<(IBig, IBig), E> {
        let value = parse_rational(numerator, denominator).map_err(E::custom)?;
        let (numerator, denominator) = value.into_parts();
        Ok((numerator, denominator.into()))
    }
    impl Serialize for AngleTarget {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let wire = match self {
                Self::DyadicRadians { bits } => Wire::DyadicRadians { bits: *bits },
                Self::RationalPi {
                    numerator,
                    denominator,
                } => {
                    let (numerator, denominator) = pair::<S::Error>(numerator, denominator)?;
                    Wire::RationalPi {
                        numerator,
                        denominator,
                    }
                }
                Self::AffinePi {
                    radians_numerator,
                    radians_denominator,
                    pi_numerator,
                    pi_denominator,
                } => {
                    let (radians_numerator, radians_denominator) =
                        pair::<S::Error>(radians_numerator, radians_denominator)?;
                    let (pi_numerator, pi_denominator) =
                        pair::<S::Error>(pi_numerator, pi_denominator)?;
                    Wire::AffinePi {
                        radians_numerator,
                        radians_denominator,
                        pi_numerator,
                        pi_denominator,
                    }
                }
            };
            wire.serialize(serializer)
        }
    }
    impl<'de> Deserialize<'de> for AngleTarget {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            match Wire::deserialize(deserializer)? {
                Wire::DyadicRadians { bits } => Ok(Self::DyadicRadians { bits }),
                Wire::RationalPi {
                    numerator,
                    denominator,
                } => {
                    let (numerator, denominator) =
                        decoded_pair::<D::Error>(&numerator, &denominator)?;
                    Ok(Self::RationalPi {
                        numerator,
                        denominator,
                    })
                }
                Wire::AffinePi {
                    radians_numerator,
                    radians_denominator,
                    pi_numerator,
                    pi_denominator,
                } => {
                    let (radians_numerator, radians_denominator) =
                        decoded_pair::<D::Error>(&radians_numerator, &radians_denominator)?;
                    let (pi_numerator, pi_denominator) =
                        decoded_pair::<D::Error>(&pi_numerator, &pi_denominator)?;
                    Ok(Self::AffinePi {
                        radians_numerator,
                        radians_denominator,
                        pi_numerator,
                        pi_denominator,
                    })
                }
            }
        }
    }
}
