use std::iter::Peekable;
use std::str::Chars;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    Text,
    Number,
    Duration,
    Time,
}

#[derive(Clone, Copy, PartialEq)]
enum Token {
    Number {
        value: f64,
        digits: usize,
        decimal: bool,
    },
    Colon,
    Plus,
    Minus,
    Times,
    Over,
    Hours,
    Minutes,
    Morning,
    Afternoon,
    Partial,
}

#[derive(Clone, Copy)]
enum Quantity {
    Count(f64),
    Span(f64),
}

impl Entry {
    pub fn permits(self, current: &str, candidate: &str) -> bool {
        self.typeable(candidate) || !self.typeable(current)
    }

    pub fn parse(self, text: &str) -> Option<i32> {
        let tokens = self.tokens(text)?;

        if !self.shaped(&tokens) || tokens.contains(&Token::Partial) {
            return None;
        }

        match self {
            Self::Text => None,
            Self::Number | Self::Duration => Self::total(&tokens),
            Self::Time => Self::moment(&tokens),
        }
    }

    fn typeable(self, text: &str) -> bool {
        self == Self::Text || self.tokens(text).is_some_and(|tokens| self.shaped(&tokens))
    }

    fn tokens(self, text: &str) -> Option<Vec<Token>> {
        let text = text.to_lowercase();
        let mut characters = text.chars().peekable();
        let mut tokens = Vec::new();

        while let Some(character) = characters.peek().copied() {
            let token = match character {
                '0'..='9' | '.' => Self::number(&mut characters)?,
                _ if character.is_alphabetic() => self.word(&mut characters)?,
                _ => {
                    characters.next();

                    match character {
                        ':' => Token::Colon,
                        '+' => Token::Plus,
                        '-' => Token::Minus,
                        '*' => Token::Times,
                        '/' => Token::Over,
                        _ if character.is_whitespace() => continue,
                        _ => return None,
                    }
                }
            };

            if !self.admits(token) {
                return None;
            }

            tokens.push(token);
        }

        Some(tokens)
    }

    fn number(characters: &mut Peekable<Chars>) -> Option<Token> {
        let mut value = 0.0;
        let mut digits = 0;
        let mut decimal = false;
        let mut place = 1.0;

        while let Some(character) = characters
            .peek()
            .copied()
            .filter(|next| next.is_ascii_digit() || *next == '.')
        {
            characters.next();

            match character.to_digit(10) {
                Some(digit) if decimal => {
                    place /= 10.0;
                    value += f64::from(digit) * place;
                    digits += 1;
                }
                Some(digit) => {
                    value = value * 10.0 + f64::from(digit);
                    digits += 1;
                }
                None if decimal => return None,
                None => decimal = true,
            }
        }

        Some(Token::Number {
            value,
            digits,
            decimal,
        })
    }

    fn word(self, characters: &mut Peekable<Chars>) -> Option<Token> {
        let mut word = String::new();

        while let Some(character) = characters
            .peek()
            .copied()
            .filter(|next| next.is_alphabetic())
        {
            word.push(character);
            characters.next();
        }

        let mut partial = false;

        for (spelling, token) in Self::WORDS {
            if !self.admits(token) || !spelling.starts_with(&word) {
                continue;
            }

            if spelling == word {
                return Some(token);
            }

            partial = true;
        }

        partial.then_some(Token::Partial)
    }

    const WORDS: [(&str, Token); 12] = [
        ("am", Token::Morning),
        ("pm", Token::Afternoon),
        ("h", Token::Hours),
        ("hr", Token::Hours),
        ("hrs", Token::Hours),
        ("hour", Token::Hours),
        ("hours", Token::Hours),
        ("m", Token::Minutes),
        ("min", Token::Minutes),
        ("mins", Token::Minutes),
        ("minute", Token::Minutes),
        ("minutes", Token::Minutes),
    ];

    fn admits(self, token: Token) -> bool {
        match self {
            Self::Text => true,
            Self::Number => matches!(
                token,
                Token::Number { .. } | Token::Plus | Token::Minus | Token::Times | Token::Over
            ),
            Self::Duration => matches!(
                token,
                Token::Number { .. }
                    | Token::Plus
                    | Token::Minus
                    | Token::Times
                    | Token::Over
                    | Token::Hours
                    | Token::Minutes
                    | Token::Partial
            ),
            Self::Time => !matches!(token, Token::Times | Token::Over),
        }
    }

    fn shaped(self, tokens: &[Token]) -> bool {
        match self {
            Self::Text => true,
            Self::Number => !Self::doubled(tokens) && Self::integral(tokens),
            Self::Duration => !Self::doubled(tokens),
            Self::Time => Self::clocked(tokens),
        }
    }

    fn integral(tokens: &[Token]) -> bool {
        !tokens
            .iter()
            .any(|token| matches!(token, Token::Number { decimal: true, .. }))
    }

    fn clocked(tokens: &[Token]) -> bool {
        let (time, adjustments) = tokens.split_at(Self::break_point(tokens));

        Self::timed(time) && Self::adjusted(adjustments)
    }

    fn break_point(tokens: &[Token]) -> usize {
        tokens
            .iter()
            .position(|token| matches!(token, Token::Plus | Token::Minus))
            .unwrap_or(tokens.len())
    }

    fn doubled(tokens: &[Token]) -> bool {
        tokens
            .windows(2)
            .any(|pair| Self::operator(pair[0]) && Self::operator(pair[1]))
    }

    fn total(tokens: &[Token]) -> Option<i32> {
        let (mut total, mut rest) = Self::term(tokens)?;

        while let Some(token) = rest.first().copied() {
            let sign = match token {
                Token::Minus => -1.0,
                Token::Plus | Token::Number { .. } => 1.0,
                _ => return None,
            };
            let (term, tail) = Self::term(if Self::operator(token) {
                &rest[1..]
            } else {
                rest
            })?;

            total = total.sum(term, sign);
            rest = tail;
        }

        Self::rounded(total.amount())
    }

    fn rounded(minutes: f64) -> Option<i32> {
        let minutes = minutes.round();

        (minutes.is_finite() && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&minutes))
            .then_some(minutes as i32)
    }

    fn moment(tokens: &[Token]) -> Option<i32> {
        let (time, adjustments) = tokens.split_at(Self::break_point(tokens));
        let mut minutes = f64::from(Self::clock(time)?);
        let mut rest = adjustments;

        while let Some(token) = rest.first().copied() {
            let sign = match token {
                Token::Plus => 1.0,
                Token::Minus => -1.0,
                _ => return None,
            };
            let (shift, tail) = Self::shift(&rest[1..])?;

            minutes += sign * shift;
            rest = tail;
        }

        Some(Self::rounded(minutes)?.rem_euclid(24 * 60))
    }

    fn clock(tokens: &[Token]) -> Option<i32> {
        let [
            Token::Number {
                value: hour,
                digits: 1..=2,
                decimal: false,
            },
            rest @ ..,
        ] = tokens
        else {
            return None;
        };
        let hour = *hour as i32;
        let (minute, rest) = match rest {
            [
                Token::Colon,
                Token::Number {
                    value,
                    digits: 2,
                    decimal: false,
                },
                rest @ ..,
            ] => (*value as i32, rest),
            [Token::Colon, ..] => return None,
            _ => (0, rest),
        };
        let hour = match rest {
            [] => (0..24).contains(&hour).then_some(hour)?,
            [Token::Morning] => (1..=12).contains(&hour).then_some(hour % 12)?,
            [Token::Afternoon] => (1..=12).contains(&hour).then_some(hour % 12 + 12)?,
            _ => return None,
        };

        (0..60).contains(&minute).then_some(hour * 60 + minute)
    }

    fn shift(tokens: &[Token]) -> Option<(f64, &[Token])> {
        let (first, mut rest) = Self::factor(tokens)?;
        let mut shift = first.amount();

        while matches!(rest.first(), Some(Token::Number { .. })) {
            let (next, tail) = Self::factor(rest)?;

            shift += next.amount();
            rest = tail;
        }

        Some((shift, rest))
    }

    fn term(tokens: &[Token]) -> Option<(Quantity, &[Token])> {
        let (mut term, mut rest) = Self::factor(tokens)?;

        while let Some(token) = rest.first().copied() {
            if !matches!(token, Token::Times | Token::Over) {
                break;
            }

            let (factor, tail) = Self::factor(&rest[1..])?;

            term = match token {
                Token::Times => term.product(factor)?,
                _ => term.ratio(factor)?,
            };
            rest = tail;
        }

        Some((term, rest))
    }

    fn factor(tokens: &[Token]) -> Option<(Quantity, &[Token])> {
        let [
            Token::Number {
                value, digits: 1.., ..
            },
            rest @ ..,
        ] = tokens
        else {
            return None;
        };

        Some(match rest {
            [Token::Hours, rest @ ..] => (Quantity::Span(value * 60.0), rest),
            [Token::Minutes, rest @ ..] => (Quantity::Span(*value), rest),
            _ => (Quantity::Count(*value), rest),
        })
    }

    fn adjusted(tokens: &[Token]) -> bool {
        !Self::doubled(tokens)
            && tokens.iter().all(|token| {
                matches!(
                    token,
                    Token::Number { .. }
                        | Token::Plus
                        | Token::Minus
                        | Token::Hours
                        | Token::Minutes
                        | Token::Partial
                )
            })
    }

    fn timed(tokens: &[Token]) -> bool {
        let mut rest = tokens;

        if let [
            Token::Number {
                digits, decimal, ..
            },
            tail @ ..,
        ] = rest
        {
            if *digits > 2 || *decimal {
                return false;
            }

            rest = tail;
        }

        if let [Token::Colon, tail @ ..] = rest {
            rest = tail;

            if let [
                Token::Number {
                    digits, decimal, ..
                },
                tail @ ..,
            ] = rest
            {
                if *digits > 2 || *decimal {
                    return false;
                }

                rest = tail;
            }
        }

        matches!(
            rest,
            [] | [Token::Morning] | [Token::Afternoon] | [Token::Partial]
        )
    }

    fn operator(token: Token) -> bool {
        matches!(
            token,
            Token::Plus | Token::Minus | Token::Times | Token::Over
        )
    }
}

impl Quantity {
    fn sum(self, other: Self, sign: f64) -> Self {
        let value = self.amount() + sign * other.amount();

        match (self, other) {
            (Self::Count(_), Self::Count(_)) => Self::Count(value),
            _ => Self::Span(value),
        }
    }

    fn product(self, other: Self) -> Option<Self> {
        let value = self.amount() * other.amount();

        match (self, other) {
            (Self::Span(_), Self::Span(_)) => None,
            (Self::Count(_), Self::Count(_)) => Some(Self::Count(value)),
            _ => Some(Self::Span(value)),
        }
    }

    fn ratio(self, other: Self) -> Option<Self> {
        let Self::Count(divisor) = other else {
            return None;
        };
        let value = self.amount() / divisor;

        Some(match self {
            Self::Count(_) => Self::Count(value),
            Self::Span(_) => Self::Span(value),
        })
    }

    fn amount(self) -> f64 {
        match self {
            Self::Count(value) | Self::Span(value) => value,
        }
    }
}
