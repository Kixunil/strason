// Stringly-Typed JSON Library for Rust
// Written in 2015 by
//   Andrew Poelstra <apoelstra@wpsoftware.net>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the CC0 Public Domain Dedication
// along with this software.
// If not, see <http://creativecommons.org/publicdomain/zero/1.0/>.
//

//! # Parsing support
//!

use std::{error, char, fmt, io, num};

use {Json, JsonInner};

/// The type of a Json parsing error
#[derive(Debug)]
pub enum ErrorType {
    /// Syntax error interpreting Json
    Other(String),
    /// Missing field interpreting Json
    MissingField(&'static str),
    /// Unknown field
    UnknownField(String),
    /// Expected a string, got something else
    ExpectedString,
    /// end-of-file reached before json was complete
    UnexpectedEOF,
    /// bad character encountered when parsing some data
    UnexpectedCharacter(char),
    /// a number contained a bad or misplaced character
    MalformedNumber,
    /// an escape sequence was invalid
    MalformedEscape,
    /// an identifier was given that has no meaning
    UnknownIdent,
    /// a unicode codepoint constant was malformed
    Unicode(num::ParseIntError),
    /// UTF-16 sequence with unpaired surrogate
    UnpairedSurrogate,
    /// some sort of IO error
    Io(io::Error)
}

impl From<num::ParseIntError> for ErrorType {
    fn from(e: num::ParseIntError) -> ErrorType { ErrorType::Unicode(e) }
}

impl From<io::Error> for ErrorType {
    fn from(e: io::Error) -> ErrorType { ErrorType::Io(e) }
}

/// A macro which acts like try! but attaches line/column info to the error
macro_rules! try_at(
    ($s:expr, $e:expr) => (
        match $e {
            Ok(x) => x,
            Err(e) => {
                return Err($s.error_at(From::from(e)));
            }
        }
    )
);


/// A Json parsing error
#[derive(Debug)]
pub struct Error {
    line: usize,
    col: usize,
    error: ErrorType
}

impl From<ErrorType> for Error {
    fn from(e: ErrorType) -> Error { Error { line: 1, col: 1, error: e } }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.error {
            ErrorType::UnexpectedCharacter(c) => write!(f, "{}:{}: unexpected character {}", self.line, self.col, c),
            ErrorType::Io(ref e) => write!(f, "{}:{}: {}", self.line, self.col, e),
            ErrorType::Unicode(ref e) => write!(f, "{}:{}: {}", self.line, self.col, e),
            ErrorType::MissingField(ref s) => write!(f, "missing field `{}`", s),
            ErrorType::UnknownField(ref s) => write!(f, "unknown field `{}`", s),
            ErrorType::Other(ref s) => write!(f, "syntax error: {}", s),
            _ => write!(f, "{}:{}: {}", self.line, self.col, error::Error::description(self))
        }
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&error::Error> {
        match self.error {
            ErrorType::Io(ref e) => Some(e),
            ErrorType::Unicode(ref e) => Some(e),
            _ => None
        }
    }

    fn description(&self) -> &str {
        match self.error {
            ErrorType::ExpectedString => "expected string",
            ErrorType::UnexpectedEOF => "unexpected eof",
            ErrorType::UnexpectedCharacter(_) => "bad character",
            ErrorType::MalformedEscape => "bad escape",
            ErrorType::MalformedNumber => "malformed number",
            ErrorType::UnknownIdent => "unknown ident",
            ErrorType::Unicode(ref e) => error::Error::description(e),
            ErrorType::UnpairedSurrogate => "UTF-16 unpaired surrogate",
            ErrorType::Io(ref e) => error::Error::description(e),
            ErrorType::MissingField(_) => "missing field",
            ErrorType::UnknownField(_) => "unknown field",
            ErrorType::Other(_) => "syntax/other error",
        }
    }
}

/// A structure capable of parsing binary ASCII data into a "JSON object",
/// which is simply a tree of strings. Further parsing should be done by
/// other layers.
pub struct Parser<I: Iterator<Item=io::Result<u8>>> {
    iter: I,
    peek: Option<u8>,
    line: usize,
    col: usize
}

impl<I: Iterator<Item=io::Result<u8>>> Iterator for Parser<I>  {
    type Item = io::Result<u8>;

    fn next(&mut self) -> Option<io::Result<u8>> {
        match self.peek.take() {
            Some(ch) => Some(Ok(ch)),
            None => {
                match self.iter.next() {
                    None => None,
                    Some(Err(e)) => Some(Err(e)),
                    Some(Ok(ch)) => {
                        if ch == b'\n' {
                            self.col = 0;
                            self.line += 1;
                        } else {
                            self.col += 1;
                        }
                        self.peek = Some(ch);
                        Some(Ok(ch))
                    }
                }
            }
        }
    }
}

impl<I: Iterator<Item=io::Result<u8>>> Parser<I> {
    /// Construct a new parser, given a byte iterator as input
    pub fn new(iter: I) -> Parser<I> {
        Parser {
            iter: iter,
            peek: None,
            line: 1,
            col: 0,
        }
    }

    fn error_at(&self, ty: ErrorType) -> Error {
        Error {
            line: self.line,
            col: self.col,
            error: ty,
        }
    }

    fn peek(&mut self) -> Result<Option<u8>, Error> {
        match self.next() {
            Some(Ok(ch)) => {
                self.peek = Some(ch);
                Ok(Some(ch))
            }
            Some(Err(e)) => Err(self.error_at(ErrorType::Io(e))),
            None => Ok(None),
        }
    }

    fn peek_noeof(&mut self) -> Result<u8, Error> {
        match self.peek() {
            Ok(Some(c)) => Ok(c),
            Ok(None) => Err(self.error_at(ErrorType::UnexpectedEOF)),
            Err(e) => Err(e)
        }
    }

    fn eat(&mut self) { self.peek = None; }

    fn eat_whitespace(&mut self) -> Result<(), Error> {
        loop {
            match self.peek()? {
                Some(b' ') | Some(b'\n') | Some(b'\r') => {
                    self.eat();
                }
                _ => { return Ok(()); }
            }
        }
    }

    fn eat_ident(&mut self, ident: &'static str) -> Result<(), Error> {
        for c in ident.bytes() {
            if self.peek()? == Some(c) {
                self.eat();
            } else {
                return Err(self.error_at(ErrorType::UnknownIdent));
            }
        }
        Ok(())
    }

    fn parse_number(&mut self) -> Result<String, Error> {
        #[derive(PartialEq)]
        enum State { Start, ZeroStart, PreDecimal, PostDecimal, InExp, PastExp }

        let mut ret = String::new();
        let mut state = State::Start;
        while let Some(c) = self.peek()? {
            match c {
                b'+' => {
                    if state == State::InExp {
                        state = State::PastExp;
                    } else {
                        return Err(self.error_at(ErrorType::UnexpectedCharacter('+')));
                    }
                }
                b'-' => {
                    if state == State::InExp {
                        state = State::PastExp;
                    } else if state != State::Start {
                        return Err(self.error_at(ErrorType::UnexpectedCharacter('-')));
                    }
                }
                b'0' ... b'9' => {
                    if state == State::Start {
                        if c == b'0' {
                            state = State::ZeroStart
                        } else {
                            state = State::PreDecimal;
                        }
                    // Can't start a number with 0, except 0 itself and 0.xyz
                    } else if state == State::ZeroStart {
                        return Err(self.error_at(ErrorType::MalformedNumber));
                    }
                }
                b'.' => {
                    if state == State::PreDecimal || state == State::ZeroStart {
                        state = State::PostDecimal;
                    } else {
                        return Err(self.error_at(ErrorType::MalformedNumber));
                    }
                }
                b' ' | b'\r' | b'\n' | b'}' | b']' | b',' | b':' => {
                    break;
                }
                b'e' | b'E' => {
                    // e, E, e+, E+, e-, E- may appear at the end of a number. never at the start
                    if state == State::ZeroStart ||
                       state == State::PreDecimal ||
                       state == State::PostDecimal {
                        state = State::InExp;
                    } else {
                        return Err(self.error_at(ErrorType::MalformedNumber));
                    }
                }
                x => {
                    return Err(self.error_at(ErrorType::UnexpectedCharacter(x as char)));
                }
            }
            ret.push(c as char);
            self.eat();
        }
        if state == State::Start {
            Err(self.error_at(ErrorType::MalformedNumber))
        } else {
            Ok(ret)
        }
    }

    /// Consume a string, assuming the first character has been vetted to be '"'.
    fn parse_string(&mut self) -> Result<String, Error> {
        #[derive(PartialEq)]
        enum State { Start, Scanning, Escaping, Done }

        let mut ret = String::new();
        let mut state = State::Start;
        while let Some(mut c) = self.peek()? {
            match c {
                b'"' => {
                    match state {
                        State::Start => { state = State::Scanning; self.eat(); continue; }
                        State::Scanning => { self.eat(); state = State::Done; break; }
                        State::Escaping => { state = State::Scanning; }
                        State::Done => unreachable!()
                    }
                }
                b'\\' => {
                    match state {
                        State::Start => { return Err(self.error_at(ErrorType::ExpectedString)); }
                        State::Scanning => { state = State::Escaping; self.eat(); continue; }
                        State::Escaping => { state = State::Scanning; }
                        State::Done => unreachable!()
                    }
                }
                _ => {
                    match state {
                        State::Start => {
                            return Err(self.error_at(ErrorType::ExpectedString));
                        }
                        State::Scanning => {
                            // Do nothing -- after the match we will push this character onto the buffer
                        }
                        State::Escaping => {
                            c = match c {
                                b'b' => 7,
                                b'f' => 12,
                                b'n' => b'\n',
                                b'r' => b'\r',
                                b't' => b'\t',
                                b'/' => b'/',
                                b'\\' => unreachable!(),  // covered above in the main b'\\' branch
                                b'u' => {
                                    // Read as many \uXXXX's in a row as we can, then parse them all as
                                    // UTF16, according to ECMA 404 p10
                                    let mut utf16_be: Vec<u16> = vec![];
                                    loop {
                                        // Parse codepoint
                                        self.eat();
                                        let mut num_str = String::new();
                                        num_str.push(self.peek_noeof()? as char); self.eat();
                                        num_str.push(self.peek_noeof()? as char); self.eat();
                                        num_str.push(self.peek_noeof()? as char); self.eat();
                                        num_str.push(self.peek_noeof()? as char); self.eat();
                                        utf16_be.push(try_at!(self, u16::from_str_radix(&num_str[..], 16)));
                                        // Check if another codepoint follows
                                        if self.peek()? == Some(b'\\') {
                                            self.eat();
                                            if self.peek()? != Some(b'u') {
                                                state = State::Escaping;
                                                break;
                                            }
                                        } else {
                                            state = State::Scanning;
                                            break;
                                        }
                                    }

                                    for ch in char::decode_utf16(utf16_be.iter().cloned()) {
                                        match ch {
                                            Ok(ch) => ret.push(ch),
                                            Err(_) => return Err(self.error_at(ErrorType::UnpairedSurrogate))
                                        }
                                    }
                                    continue;
                                }
                                _ => { return Err(self.error_at(ErrorType::MalformedEscape)); }
                            };
                            state = State::Scanning;
                        }
                        State::Done => unreachable!()
                    }
                }
            }
            ret.push(c as char);
            self.eat();
        }
        if state == State::Done {
            Ok(ret)
        } else {
            Err(self.error_at(ErrorType::UnexpectedEOF))
        }
    }

    /// Consume the internal iterator and produce a Json object
    pub fn parse(&mut self) -> Result<Json, super::Error> {
        self.eat_whitespace()?;

        let first_ch = match self.peek() {
            Ok(Some(c)) => c,
            Ok(None) => return Err(From::from(self.error_at(ErrorType::UnexpectedEOF))),
            Err(e) => return Err(From::from(e)),
        };

        match first_ch {
            // keywords
            b'n' => {
                self.eat_ident("null")?;
                Ok(Json(JsonInner::Null))
            }
            b't' => {
                self.eat_ident("true")?;
                Ok(Json(JsonInner::Bool(true)))
            }
            b'f' => {
                self.eat_ident("false")?;
                Ok(Json(JsonInner::Bool(false)))
            }
            // numbers
            b'-' | b'0' ... b'9' => {
                Ok(Json(JsonInner::Number(self.parse_number()?)))
            }
            // strings
            b'"' | b'\'' => {
                Ok(Json(JsonInner::String(self.parse_string()?)))
            }
            // arrays
            b'[' => {
                self.eat();
                let mut ret = vec![];
                loop {
                    self.eat_whitespace()?;
                    if !(ret.is_empty() && self.peek_noeof()? == b']') {
                        ret.push(self.parse()?);
                        self.eat_whitespace()?;
                    }
                    match self.peek_noeof()? {
                        b',' => { self.eat(); }
                        b']' => { self.eat(); break; }
                        _ => { return Err(From::from(self.error_at(ErrorType::UnknownIdent))); }
                    }
                }
                Ok(Json(JsonInner::Array(ret)))
            }
            // objects TODO
            b'{' => {
                self.eat();
                let mut ret = vec![];
                loop {
                    self.eat_whitespace()?;
                    // special-case {}
                    if ret.is_empty() && self.peek_noeof()? == b'}' {
                        self.eat();
                        break;
                    }
                    // parse key
                    let key = self.parse_string()?;
                    self.eat_whitespace()?;
                    // parse : separator
                    let sep_ch = self.peek_noeof()?;
                    if sep_ch == b':' {
                        self.eat();
                        self.eat_whitespace()?;
                    } else {
                        return Err(From::from(self.error_at(ErrorType::UnexpectedCharacter(sep_ch as char))));
                    }
                    // parse value
                    let val = self.parse()?;
                    ret.push((key, val));
                    self.eat_whitespace()?;
                    // parse , separator
                    match self.peek_noeof()? {
                        b',' => { self.eat(); },
                        b'}' /* { */ => { self.eat(); break; }
                        x => { return Err(From::from(self.error_at(ErrorType::UnexpectedCharacter(x as char)))); }
                    }
                }
                Ok(Json(JsonInner::Object(ret)))
            }
            _ => Err(From::from(self.error_at(ErrorType::UnknownIdent)))
        }
    }
}

#[cfg(test)]
mod tests {
    use {Json, JsonInner};
    use {Error, ErrorInner};

    macro_rules! jnull( () => (Json(JsonInner::Null)) );
    macro_rules! jbool( ($e:expr) => (Json(JsonInner::Bool($e))) );
    macro_rules! jnum( ($e:expr) => (Json(JsonInner::Number($e.to_owned()))) );
    macro_rules! jstr( ($e:expr) => (Json(JsonInner::String($e.to_owned()))) );
    macro_rules! jarr( ($($e:expr),*) => (Json(JsonInner::Array(vec![$($e),*]))) );
    macro_rules! jobj( ($($k:expr => $v:expr),*) => ({
        let mut vec = vec![];
        &mut vec;  /* dummy "use as mut" to avoid errors in case of no inserts */
        $(
            vec.push(($k.to_owned(), $v));
        )*
        Json(JsonInner::Object(vec))
    }) );

    #[test]
    fn test_primitives() {
        assert_eq!(Json::from_str("null").unwrap(), jnull!());
        assert_eq!(Json::from_str("  true  ").unwrap(), jbool!(true));
        assert_eq!(Json::from_str(" false ").unwrap(), jbool!(false));

        assert_eq!(Json::from_str("\"\\n\\r\\t\\b\\f \\\\ \\/\"").unwrap(), jstr!("\n\r\t\u{7}\u{c} \\ /"));
        assert_eq!(Json::from_str("\"\\\"\"").unwrap(), jstr!("\""));
        assert_eq!(Json::from_str(" \"string\"").unwrap(), jstr!("string"));
        assert_eq!(Json::from_str("\"i've \\\"ed this\"").unwrap(), jstr!("i've \"ed this"));

        assert_eq!(Json::from_str(" 0").unwrap(), jnum!("0"));
        assert_eq!(Json::from_str("-0").unwrap(), jnum!("-0"));
        assert_eq!(Json::from_str("  101").unwrap(), jnum!("101"));
        assert_eq!(Json::from_str("101.99").unwrap(), jnum!("101.99"));
        assert_eq!(Json::from_str("-101.99  ").unwrap(), jnum!("-101.99"));
        assert_eq!(Json::from_str("-10e55").unwrap(), jnum!("-10e55"));
        assert_eq!(Json::from_str("-10.1e55").unwrap(), jnum!("-10.1e55"));
        assert_eq!(Json::from_str("-10e+55").unwrap(), jnum!("-10e+55"));
        assert_eq!(Json::from_str("-10e-55").unwrap(), jnum!("-10e-55"));
        assert_eq!(Json::from_str("-1E+5").unwrap(), jnum!("-1E+5"));
        assert_eq!(Json::from_str("-1E-5").unwrap(), jnum!("-1E-5"));

        assert!(Json::from_str("").is_err());
        assert!(Json::from_str("gibberish").is_err());
        assert!(Json::from_str("\"\\c\"").is_err());
        assert!(Json::from_str("\"\\u\"").is_err());
        assert!(Json::from_str("\"\\").is_err());
        assert!(Json::from_str(".5").is_err());
        assert!(Json::from_str("9.5.5").is_err());
        assert!(Json::from_str("-").is_err());
        assert!(Json::from_str("+").is_err());
        assert!(Json::from_str("+1").is_err());
        assert!(Json::from_str("1e2.5").is_err());
        assert!(Json::from_str("1e2e5").is_err());
        assert!(Json::from_str("0123").is_err());
        assert!(Json::from_str("3f").is_err());
        assert!(Json::from_str("00").is_err());
        assert!(Json::from_str("2-3").is_err());
        assert!(Json::from_str("2+3").is_err());
    }

    #[test]
    fn test_utf16() {
        assert!(Json::from_str("\"\\u123\"").is_err());
        // Following two tests are invalid UCS-2 but valid UTF-16. We do the wrong thing
        // here (by accepting them) in order to stick with only stdlib functions, since
        // the alternative is to add dependencies on very heavy string-parsing machinery.
        //assert!(Json::from_str("\"\\ud800\"").is_err());
        //assert!(Json::from_str("\"\\udd1e\\ud834\"").is_err());
        assert!(Json::from_str("\"\\uf+ff\"").is_err());
        assert!(Json::from_str("\"").is_err());
        assert_eq!(Json::from_str(" \"\\u0020\"").unwrap(), jstr!(" "));
        assert_eq!(Json::from_str(" \"\\uffff\\t\"").unwrap(), jstr!("\u{ffff}\t"));
        assert_eq!(Json::from_str(" \"\\ud834\\uDD1E\"").unwrap(), jstr!("\u{1d11e}"));
    }

    #[test]
    fn test_array() {
        assert_eq!(Json::from_str("[]").unwrap(), jarr![]);
        assert_eq!(Json::from_str("[\"1\"]").unwrap(), jarr![jstr!("1")]);
        assert_eq!(Json::from_str("[\"1\", 2]").unwrap(), jarr![jstr!("1"), jnum!("2")]);

        assert_eq!(Json::from_str("[true, [false, 2], 3]").unwrap(),
                   jarr![jbool!(true), jarr![jbool!(false), jnum!("2")], jnum!("3")]);
        assert_eq!(Json::from_str("[[[[[]]]]]").unwrap(), jarr![jarr![jarr![jarr![jarr![]]]]]);

        assert!(Json::from_str("[").is_err());
        assert!(Json::from_str("]").is_err());
        assert!(Json::from_str("[1 2]").is_err());
        assert!(Json::from_str("[,1]").is_err());
        assert!(Json::from_str("[1,]").is_err());
        assert!(Json::from_str("[,1,2]").is_err());
        assert!(Json::from_str("[1,,2]").is_err());
        assert!(Json::from_str("[1,2,]").is_err());
    }

    #[test]
    fn test_object() {
        assert_eq!(Json::from_str("{}").unwrap(), jobj![]);
        assert_eq!(Json::from_str("{\"key\": \"val\"}").unwrap(), jobj!["key" => jstr!("val")]);
        assert_eq!(Json::from_str("{\"key\": false}").unwrap(), jobj!["key" => jbool!(false)]);
        assert_eq!(Json::from_str("{\"key\": []}").unwrap(), jobj!["key" => jarr![]]);

        assert_eq!(Json::from_str("{\"key\": 1234}").unwrap(), jobj!["key" => jnum!("1234")]);

        assert_eq!(Json::from_str("{\"key1\": \"val\", \"key2\": \"val\"}").unwrap(), jobj!["key1" => jstr!("val"), "key2" => jstr!("val")]);
        assert_eq!(Json::from_str("{\"key\": \"val\", \"key\": \"val2\"}").unwrap(), jobj!["key" => jstr!("val"), "key" => jstr!("val2")]);

        assert!(Json::from_str("{{}}").is_err());
        assert!(Json::from_str("{,}").is_err());
        assert!(Json::from_str("{:}").is_err());
        assert!(Json::from_str("{\\\"}").is_err());
        assert!(Json::from_str("{\"key\" \"val\"}").is_err());
        assert!(Json::from_str("{\"key\": \"val\" \"val2\"}").is_err());
        assert!(Json::from_str("{{\"key\": \"val\"}}").is_err());
        assert!(Json::from_str("{null: \"val\"}").is_err());
        assert!(Json::from_str("{true: \"val\"}").is_err());
        assert!(Json::from_str("{false: \"val\"}").is_err());
        assert!(Json::from_str("{[]: \"val\"}").is_err());
        assert!(Json::from_str("{10: \"val\"}").is_err());
        assert!(Json::from_str("{: \"val\"}").is_err());
        assert!(Json::from_str("{\"key\": }").is_err());
        assert!(Json::from_str("{\"key1\": , \"key2\": \"val\"}").is_err());
        assert!(Json::from_str("{\"key1\": \"val\", \"key2\":}").is_err());
        assert!(Json::from_str("{\"key1\": \"val\",, \"key2\":\"val\"}").is_err());
        assert!(Json::from_str("{,\"key1\": \"val\", \"key2\":\"val\"}").is_err());
        assert!(Json::from_str("{\"key1\": \"val\", \"key2\":\"val\",}").is_err());
    }

    #[test]
    fn test_error() {
        if let Err(Error(ErrorInner::Parser(e))) = Json::from_str("10+5") {
            assert_eq!(e.line, 1);
            assert_eq!(e.col, 3);
            assert_eq!(e.to_string(), "1:3: unexpected character +");
        } else {
            panic!("wrong error return type");
        }
    }
}


