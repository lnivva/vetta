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
    /// Format a `Quarter` as its short label (for example, `"Q1"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::Quarter;
    ///
    /// assert_eq!(format!("{}", Quarter::Q1), "Q1");
    /// assert_eq!(format!("{}", Quarter::Q4), "Q4");
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

    /// Parses a string into a `Quarter` (case-insensitive).
    ///
    /// Returns `Ok(Quarter::Qn)` when the input is `"Q1"`, `"Q2"`, `"Q3"`, or `"Q4"` in any letter case, and `Err` with the message `Invalid quarter: {s}` for any other input.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::str::FromStr;
    ///
    /// assert_eq!(Quarter::from_str("q2").unwrap(), Quarter::Q2);
    /// assert!(Quarter::from_str("Q5").is_err());
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