//! `text_enum!`: enums stored as `TEXT` in PostgreSQL and serialized as snake_case strings.

/// Error returned when parsing a string that is not a known variant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} `{value}`")]
pub struct UnknownVariant {
    pub kind: &'static str,
    pub value: String,
}

/// Declares a fieldless enum with a fixed textual representation for each variant.
///
/// Generates `ALL`, `as_str`, `Display`, `FromStr`, `TryFrom<String>` (used by
/// `#[sqlx(try_from = "String")]` in row types) and serde/utoipa derives using the same text.
macro_rules! text_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            serde::Serialize, serde::Deserialize, utoipa::ToSchema,
        )]
        $vis enum $name {
            $( $(#[$vmeta])* #[serde(rename = $text)] $variant ),+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// Textual representation, as stored in the database and sent over the API.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::domain::UnknownVariant;

            fn from_str(s: &str) -> Result<Self, $crate::domain::UnknownVariant> {
                match s {
                    $($text => Ok($name::$variant),)+
                    other => Err($crate::domain::UnknownVariant {
                        kind: stringify!($name),
                        value: other.to_owned(),
                    }),
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = $crate::domain::UnknownVariant;

            fn try_from(value: String) -> Result<Self, $crate::domain::UnknownVariant> {
                value.parse()
            }
        }
    };
}

pub(crate) use text_enum;

#[cfg(test)]
mod tests {
    text_enum! {
        /// Test enum.
        pub enum Color {
            Red => "red",
            DarkBlue => "dark_blue",
        }
    }

    #[test]
    fn round_trips_through_text_and_serde() {
        for color in Color::ALL {
            assert_eq!(color.as_str().parse::<Color>().unwrap(), *color);
            assert_eq!(color.to_string(), color.as_str());
            let json = serde_json::to_string(color).unwrap();
            assert_eq!(json, format!("\"{}\"", color.as_str()));
            assert_eq!(serde_json::from_str::<Color>(&json).unwrap(), *color);
        }
        assert_eq!(Color::try_from("dark_blue".to_owned()), Ok(Color::DarkBlue));
    }

    #[test]
    fn unknown_text_is_rejected() {
        let err = "green".parse::<Color>().unwrap_err();
        assert_eq!(err.to_string(), "unknown Color `green`");
        assert!(serde_json::from_str::<Color>("\"Red\"").is_err());
    }
}
