use super::{ParseError, ParseLimits, Token, TokenKind};
use crate::SourceSnapshot;

/// Lex a caller-owned snapshot without reading files or reconstructing tokens.
///
/// # Errors
/// Rejects malformed literals, unsupported characters, and resource exhaustion.
pub fn lex(source: &SourceSnapshot, limits: ParseLimits) -> Result<Vec<Token>, ParseError> {
	if source.text().len() > limits.source_bytes {
		return Err(ParseError::budget_limit(
			"source byte budget exceeded",
			crate::ResourceKind::SourceBytes,
			source.text().len(),
			limits.source_bytes,
		));
	}
	let mut cursor = source.text().char_indices().peekable();
	let mut tokens = Vec::new();
	while let Some((start, character)) = cursor.next() {
		if character.is_whitespace() {
			continue;
		}
		let mut end = start
			.checked_add(character.len_utf8())
			.ok_or_else(|| ParseError::budget("source offset overflow"))?;
		let mut spelling = String::from(character);
		if character == '/' && cursor.peek().is_some_and(|(_, c)| *c == '/') {
			for (_, c) in cursor.by_ref() {
				if c == '\n' {
					break;
				}
			}
			continue;
		}
		if character == '/' && cursor.peek().is_some_and(|(_, c)| *c == '*') {
			cursor.next();
			let mut previous = ' ';
			let mut closed = false;
			for (_, c) in cursor.by_ref() {
				if previous == '*' && c == '/' {
					closed = true;
					break;
				}
				previous = c;
			}
			if !closed {
				return Err(ParseError::syntax(
					source.span(start..end).ok(),
					"unterminated block comment",
				));
			}
			continue;
		}
		let kind = if character == '"' {
			spelling = read_string(&mut cursor, &mut end, source, start)?;
			TokenKind::String(spelling)
		} else if character.is_alphabetic() || character == '_' {
			take_while(&mut cursor, &mut end, &mut spelling, |c| {
				c.is_alphanumeric() || c == '_'
			})?;
			TokenKind::Identifier(spelling)
		} else if character.is_ascii_digit()
			|| (character == '.' && cursor.peek().is_some_and(|(_, c)| c.is_ascii_digit()))
		{
			take_while(&mut cursor, &mut end, &mut spelling, |c| {
				c.is_ascii_alphanumeric() || c == '_' || c == '.'
			})?;
			// A sign belongs to a decimal exponent only, never to ordinary addition.
			if spelling.ends_with(['e', 'E'])
				&& cursor.peek().is_some_and(|(_, c)| *c == '+' || *c == '-')
			{
				if let Some((offset, c)) = cursor.next() {
					spelling.push(c);
					end = offset
						.checked_add(1)
						.ok_or_else(|| ParseError::budget("source offset overflow"))?;
				}
				take_while(&mut cursor, &mut end, &mut spelling, |c| {
					c.is_ascii_digit() || c == '_'
				})?;
			}
			TokenKind::Number(spelling)
		} else if ";,()[]{}:@+-*/%=!~&|^<>".contains(character) {
			complete_symbol(&mut cursor, &mut end, &mut spelling, character)?;
			TokenKind::Symbol(spelling)
		} else {
			return Err(ParseError::syntax(
				source.span(start..end).ok(),
				format!("unsupported character {character:?}"),
			));
		};
		if tokens.len() >= limits.tokens {
			return Err(ParseError::budget_limit(
				"token budget exceeded",
				crate::ResourceKind::SyntaxTokens,
				tokens.len().saturating_add(1),
				limits.tokens,
			));
		}
		tokens
			.try_reserve(1)
			.map_err(|_| ParseError::budget("token allocation failed"))?;
		tokens.push(Token {
			kind,
			span: source.span(start..end).ok(),
		});
	}
	Ok(tokens)
}
fn take_while<I: Iterator<Item = (usize, char)>>(
	cursor: &mut std::iter::Peekable<I>,
	end: &mut usize,
	spelling: &mut String,
	predicate: impl Fn(char) -> bool,
) -> Result<(), ParseError> {
	while let Some((offset, c)) = cursor.peek().copied() {
		if !predicate(c) {
			break;
		}
		cursor.next();
		spelling.push(c);
		*end = offset
			.checked_add(c.len_utf8())
			.ok_or_else(|| ParseError::budget("source offset overflow"))?;
	}
	Ok(())
}

fn complete_symbol<I: Iterator<Item = (usize, char)>>(
	cursor: &mut std::iter::Peekable<I>,
	end: &mut usize,
	spelling: &mut String,
	character: char,
) -> Result<(), ParseError> {
	if let Some((offset, next)) = cursor.peek().copied() {
		let pair = format!("{character}{next}");
		if [
			"->", "+=", "-=", "*=", "/=", "%=", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>",
			"**", "&=", "|=", "^=", "++",
		]
		.contains(&pair.as_str())
		{
			spelling.push(next);
			cursor.next();
			*end = offset
				.checked_add(1)
				.ok_or_else(|| ParseError::budget("source offset overflow"))?;
			if matches!(pair.as_str(), "<<" | ">>") && cursor.peek().is_some_and(|(_, c)| *c == '=')
			{
				cursor.next();
				spelling.push('=');
				*end = end
					.checked_add(1)
					.ok_or_else(|| ParseError::budget("source offset overflow"))?;
			}
		}
	}

	Ok(())
}

fn read_string<I: Iterator<Item = (usize, char)>>(
	cursor: &mut std::iter::Peekable<I>,
	end: &mut usize,
	source: &SourceSnapshot,
	start: usize,
) -> Result<String, ParseError> {
	let mut spelling = String::new();
	let mut closed = false;
	for (offset, c) in cursor.by_ref() {
		*end = offset
			.checked_add(c.len_utf8())
			.ok_or_else(|| ParseError::budget("source offset overflow"))?;
		if c == '"' {
			closed = true;
			break;
		}
		if c == '\n' || c == '\\' {
			return Err(ParseError::syntax(
				source.span(start..*end).ok(),
				"unsupported escape or newline in string",
			));
		}
		spelling.push(c);
	}
	if !closed {
		return Err(ParseError::syntax(
			source.span(start..*end).ok(),
			"unterminated string",
		));
	}

	Ok(spelling)
}
