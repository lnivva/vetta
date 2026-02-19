use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quarter {
    Q1,
    Q2,
    Q3,
    Q4,
}

impl fmt::Display for Quarter {
    /// Writes the quarter as "Q1", "Q2", "Q3", or "Q4" to the provided formatter.
    ///
    /// # Examples
    ///
    /// ```
    /// use vetta_core::domain::Quarter;
    /// assert_eq!(format!("{}", Quarter::Q3), "Q3");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Quarter::Q1 => write!(f, "Q1"),
            Quarter::Q2 => write!(f, "Q2"),
            Quarter::Q3 => write!(f, "Q3"),
            Quarter::Q4 => write!(f, "Q4"),
        }
    }
}

impl FromStr for Quarter {
    type Err = String;

    /// Parses a string into a `Quarter`, accepting case-insensitive `"Q1"`, `"Q2"`, `"Q3"`, or `"Q4"`.
    ///
    /// Returns `Ok(Quarter)` when the input matches one of the four quarter tokens (case-insensitive).
    /// Returns `Err` with the message `Invalid quarter: {s}` when the input does not match.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    /// use vetta_core::domain::Quarter;
    ///
    /// assert_eq!(Quarter::from_str("q1").unwrap(), Quarter::Q1);
    /// assert_eq!(Quarter::from_str("Q4").unwrap(), Quarter::Q4);
    /// assert!(Quarter::from_str("not-a-quarter").is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "Q1" => Ok(Quarter::Q1),
            "Q2" => Ok(Quarter::Q2),
            "Q3" => Ok(Quarter::Q3),
            "Q4" => Ok(Quarter::Q4),
            _ => Err(format!("Invalid quarter: {}", s)),
        }
    }
}
