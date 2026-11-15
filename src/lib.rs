//! Example plugin: a calculator page.
//!
//! The user types a whole expression, such as `2 + 3 × (4 − 1)`, and `=`
//! works it out: brackets first, then multiplication and division, then
//! addition and subtraction. Brackets still open at `=` are closed there.
//! The small line shows the expression; the large one the number being
//! typed, or the result. An expression that cannot be worked out, such as
//! `2 × )` or a division by zero, shows `Error`.
//!
//! Every result also goes into a history, newest first; the page keeps the
//! last 50 entries. The history is hidden at first, as in the calculators of
//! Windows and phones: the keys have the whole page, and a quiet line above
//! the number shows the last calculation. The history button at the end of
//! that line, or Ctrl+H, opens the history beside the keys, in a split whose
//! bar the user can drag. Each entry has two halves: a click on the
//! expression puts it back on the small line to be changed and worked out
//! again, a click on the result starts again from that number. "Clear
//! history" empties it.
//!
//! Buttons and the keyboard do the same things: the page declares the keys
//! it handles, and a click and a key press both become one action name. The
//! copy button puts the shown number on the clipboard, the one permission
//! the plugin declares. The state survives a restart, the history, whether
//! it is open and where the user left the bar included: every render saves
//! it, and `open` gets it back. A state saved by the previous version keeps
//! its history, which opens closed; anything else starts afresh.
//!
//! The TypeScript twin in `examples/calculator-ts` draws the same tree for
//! the same input; the tests compare the two node by node. Both work out an
//! expression the same way (see `Parser`) and round results the same way
//! (see `format_number`), so they show the same digits.

use tessoa_plugin_sdk::pages::*;
use tessoa_plugin_sdk::prelude::*;

/// Keys the page handles while it has the focus.
const KEYS: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    ".",
    "+",
    "-",
    "*",
    "/",
    "(",
    ")",
    "=",
    "Enter",
    "Backspace",
    "Escape",
    "Delete",
    "Ctrl+c",
    "Meta+c",
    "Ctrl+h",
    "Meta+h",
];

/// Longest number the user can type, in characters.
const MAX_DIGITS: usize = 16;

/// Entries the history keeps; the oldest go first.
const MAX_HISTORY: usize = 50;

/// The first line of a saved state, so that a state saved in another
/// format is recognised and not misread.
const FORMAT: &str = "calculator 3";

/// The first line of the state the previous version saved: the same, without
/// the line about the history pane.
const FORMAT_2: &str = "calculator 2";

/// The share of the page the keys have when the history first opens.
const RATIO: f32 = 0.62;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
}

impl Op {
    fn symbol(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "−",
            Op::Mul => "×",
            Op::Div => "÷",
        }
    }

    /// How the operation is saved.
    fn code(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
        }
    }

    fn from_code(code: &str) -> Option<Op> {
        Some(match code {
            "+" => Op::Add,
            "-" => Op::Sub,
            "*" => Op::Mul,
            "/" => Op::Div,
            _ => return None,
        })
    }

    fn apply(self, a: f64, b: f64) -> f64 {
        match self {
            Op::Add => a + b,
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => a / b,
        }
    }
}

/// One piece of an expression.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// A number as it was typed, or a result carried over: digits, at most
    /// one dot, and a minus in front for a negative result.
    Num(String),
    Op(Op),
    Open,
    Close,
}

impl Token {
    /// How the token is saved.
    fn code(&self) -> &str {
        match self {
            Token::Num(n) => n,
            Token::Op(op) => op.code(),
            Token::Open => "(",
            Token::Close => ")",
        }
    }

    fn from_code(code: &str) -> Option<Token> {
        Some(match code {
            "(" => Token::Open,
            ")" => Token::Close,
            _ => match Op::from_code(code) {
                Some(op) => Token::Op(op),
                None if is_number(code) => Token::Num(code.to_string()),
                None => return None,
            },
        })
    }

    /// How the token shows in the expression.
    fn label(&self) -> &str {
        match self {
            Token::Num(n) => n,
            Token::Op(op) => op.symbol(),
            Token::Open => "(",
            Token::Close => ")",
        }
    }
}

/// Digits with at most one dot, and an optional minus in front.
fn is_number(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    digits.chars().any(|c| c.is_ascii_digit())
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.matches('.').count() <= 1
}

/// Works an expression out. A recursive descent parser: one method for each
/// level of precedence, each calling the level that binds tighter for its
/// operands.
///
/// ```text
/// expression = term { ("+" | "−") term }
/// term       = factor { ("×" | "÷") factor }
/// factor     = "−" factor | number | "(" expression ")"
/// ```
struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    /// The value of the expression, or `None` when it cannot be read.
    fn evaluate(tokens: &'a [Token]) -> Option<f64> {
        let mut parser = Parser { tokens, pos: 0 };
        let value = parser.expression()?;
        // Anything left over, such as a `)` that closes nothing, is an error.
        (parser.pos == tokens.len()).then_some(value)
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&'a Token> {
        let token = self.peek()?;
        self.pos += 1;
        Some(token)
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        while let Some(Token::Op(op @ (Op::Add | Op::Sub))) = self.peek() {
            self.pos += 1;
            value = op.apply(value, self.term()?);
        }
        Some(value)
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.factor()?;
        while let Some(Token::Op(op @ (Op::Mul | Op::Div))) = self.peek() {
            self.pos += 1;
            value = op.apply(value, self.factor()?);
        }
        Some(value)
    }

    fn factor(&mut self) -> Option<f64> {
        match self.next()? {
            Token::Op(Op::Sub) => Some(-self.factor()?),
            Token::Num(n) => n.parse().ok(),
            Token::Open => {
                let value = self.expression()?;
                match self.next()? {
                    Token::Close => Some(value),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Whether the operator at `i` is a sign in front of what follows rather
/// than a subtraction: it starts the expression, or follows another
/// operator or an opening bracket.
fn is_sign(tokens: &[Token], i: usize) -> bool {
    matches!(tokens[i], Token::Op(_)) && (i == 0 || matches!(tokens[i - 1], Token::Op(_) | Token::Open))
}

/// The expression as the page shows it: `2 + 3 × (4 − 1)`, `2 × −3`.
fn show_expression(tokens: &[Token]) -> String {
    let mut out = String::new();
    for (i, token) in tokens.iter().enumerate() {
        let glued =
            i == 0 || matches!(tokens[i - 1], Token::Open) || matches!(token, Token::Close) || is_sign(tokens, i - 1);
        if !glued {
            out.push(' ');
        }
        out.push_str(token.label());
    }
    out
}

/// The tokens of an expression as `show_expression` wrote it, or `None` for
/// anything it does not write. The two minus signs tell a number from an
/// operator: a negative result carried over starts with a hyphen, `-6`; the
/// operator is the sign `−`.
fn read_expression(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let token = match c {
            ' ' => continue,
            '(' => Token::Open,
            ')' => Token::Close,
            '+' => Token::Op(Op::Add),
            '−' => Token::Op(Op::Sub),
            '×' => Token::Op(Op::Mul),
            '÷' => Token::Op(Op::Div),
            '-' | '.' | '0'..='9' => {
                let mut n = c.to_string();
                while let Some(d) = chars.next_if(|d| d.is_ascii_digit() || *d == '.') {
                    n.push(d);
                }
                if !is_number(&n) {
                    return None;
                }
                Token::Num(n)
            }
            _ => return None,
        };
        tokens.push(token);
    }
    Some(tokens)
}

/// A result as the display shows it: rounded to ten decimals, without
/// trailing zeros, `Error` when it is not a number or too large. The rounding
/// is done by hand so that Rust and TypeScript print the same digits: both
/// round the same doubles in the same way, and the fixed-point formatting of
/// the rounded value is then exact in both.
fn format_number(x: f64) -> String {
    if !x.is_finite() || x.abs() >= 1e15 {
        return "Error".to_string();
    }
    let r = (x.abs() * 1e10).round() / 1e10;
    let mut s = format!("{:.10}", if x < 0.0 { -r } else { r });
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// The action of a declared key, named like the button that does the same.
fn key_action(key: &str) -> Option<String> {
    Some(
        match key {
            "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => return Some(format!("k{key}")),
            "." => "dot",
            "+" => "add",
            "-" => "sub",
            "*" => "mul",
            "/" => "div",
            "(" => "open",
            ")" => "close",
            "=" | "Enter" => "eq",
            "Backspace" => "back",
            "Escape" | "Delete" => "clear",
            "Ctrl+c" | "Meta+c" => "copy",
            "Ctrl+h" | "Meta+h" => "toggle-history",
            _ => return None,
        }
        .to_string(),
    )
}

/// A worked-out expression: what `=` shows, and what the history keeps.
#[derive(Debug, Clone, PartialEq)]
struct Answer {
    /// The expression as shown, brackets closed.
    expression: String,
    /// The result as shown, or `Error`.
    value: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    /// Names the entry's button. Never reused, so that a key stays with its
    /// entry when newer ones push it down.
    id: u32,
    answer: Answer,
}

#[derive(Debug, Clone, PartialEq)]
struct Calculator {
    /// The expression being typed.
    tokens: Vec<Token>,
    /// What `=` worked out, or the history entry clicked: shown until the
    /// user types again.
    answer: Option<Answer>,
    /// Newest first.
    history: Vec<Entry>,
    next_id: u32,
    /// Whether the history shows beside the keys.
    history_open: bool,
    /// The share of the page the keys have while the history shows. Kept
    /// here rather than left to tessoa, which forgets the bar of a split
    /// that leaves the page.
    ratio: f32,
}

impl Calculator {
    fn new() -> Calculator {
        Calculator {
            tokens: Vec::new(),
            answer: None,
            history: Vec::new(),
            next_id: 1,
            history_open: false,
            ratio: RATIO,
        }
    }

    /// The saved state, one item per line: the format, the expression's
    /// tokens apart by spaces, the answer (expression and value apart by a
    /// tab, or nothing), the next entry id, the history pane (`1` when open
    /// or `0`, then the ratio), then one line per entry (id, expression and
    /// value apart by tabs).
    fn encode(&self) -> String {
        let tokens: Vec<&str> = self.tokens.iter().map(Token::code).collect();
        let answer = match &self.answer {
            Some(a) => format!("{}\t{}", a.expression, a.value),
            None => String::new(),
        };
        let mut lines = vec![
            FORMAT.to_string(),
            tokens.join(" "),
            answer,
            self.next_id.to_string(),
            format!("{} {}", u8::from(self.history_open), self.ratio),
        ];
        for e in &self.history {
            lines.push(format!("{}\t{}\t{}", e.id, e.answer.expression, e.answer.value));
        }
        lines.join("\n")
    }

    /// The state `encode` saved, or the one the previous version saved, or
    /// `None` for anything else.
    fn decode(state: &[u8]) -> Option<Calculator> {
        let text = std::str::from_utf8(state).ok()?;
        let mut lines = text.split('\n');
        let current = match lines.next()? {
            FORMAT => true,
            FORMAT_2 => false,
            _ => return None,
        };
        let tokens = lines
            .next()?
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(Token::from_code)
            .collect::<Option<Vec<_>>>()?;
        let answer = match lines.next()? {
            "" => None,
            a => {
                let (expression, value) = a.split_once('\t')?;
                Some(Answer {
                    expression: expression.to_string(),
                    value: value.to_string(),
                })
            }
        };
        let next_id = lines.next()?.parse().ok()?;
        let (history_open, ratio) = if current {
            let (open, ratio) = lines.next()?.split_once(' ')?;
            let open = match open {
                "1" => true,
                "0" => false,
                _ => return None,
            };
            let ratio: f32 = ratio.parse().ok()?;
            if !(ratio > 0.0 && ratio < 1.0) {
                return None;
            }
            (open, ratio)
        } else {
            (false, RATIO)
        };
        let history = lines
            .map(|line| {
                let mut parts = line.splitn(3, '\t');
                let id = parts.next()?.parse().ok()?;
                let expression = parts.next()?.to_string();
                let value = parts.next()?.to_string();
                Some(Entry {
                    id,
                    answer: Answer { expression, value },
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Calculator {
            tokens,
            answer,
            history,
            next_id,
            history_open,
            ratio,
        })
    }

    /// The large line: the result, or the last number typed.
    fn display(&self) -> &str {
        if let Some(a) = &self.answer {
            return &a.value;
        }
        self.tokens
            .iter()
            .rev()
            .find_map(|t| match t {
                Token::Num(n) => Some(n.as_str()),
                _ => None,
            })
            .unwrap_or("0")
    }

    /// The small line: the expression being typed, or the one worked out.
    fn expression(&self) -> String {
        match &self.answer {
            Some(a) => format!("{} =", a.expression),
            None => show_expression(&self.tokens),
        }
    }

    /// A number or a bracket typed after an answer starts a new expression.
    fn start_over_after_answer(&mut self) {
        if self.answer.take().is_some() {
            self.tokens.clear();
        }
    }

    /// A number or an opening bracket right after a closing bracket means a
    /// multiplication: `(1 + 2)4` is `(1 + 2) × 4`. The `×` is put in, so
    /// that the expression shows what is worked out.
    fn multiply_after_close(&mut self) {
        if matches!(self.tokens.last(), Some(Token::Close)) {
            self.tokens.push(Token::Op(Op::Mul));
        }
    }

    fn digit(&mut self, d: &str) {
        self.start_over_after_answer();
        self.multiply_after_close();
        match self.tokens.last_mut() {
            Some(Token::Num(n)) if n == "0" => *n = d.to_string(),
            Some(Token::Num(n)) => {
                if n.trim_start_matches('-').len() < MAX_DIGITS {
                    n.push_str(d);
                }
            }
            _ => self.tokens.push(Token::Num(d.to_string())),
        }
    }

    fn dot(&mut self) {
        self.start_over_after_answer();
        self.multiply_after_close();
        match self.tokens.last_mut() {
            Some(Token::Num(n)) => {
                if !n.contains('.') && n.trim_start_matches('-').len() < MAX_DIGITS {
                    n.push('.');
                }
            }
            _ => self.tokens.push(Token::Num("0.".to_string())),
        }
    }

    /// An opening bracket right after a number multiplies too: `2(3)` is
    /// `2 × (3)`.
    fn open(&mut self) {
        self.start_over_after_answer();
        self.multiply_after_close();
        if matches!(self.tokens.last(), Some(Token::Num(_))) {
            self.tokens.push(Token::Op(Op::Mul));
        }
        self.tokens.push(Token::Open);
    }

    fn close(&mut self) {
        self.start_over_after_answer();
        self.tokens.push(Token::Close);
    }

    /// An operator after an answer goes on from its result.
    fn operator(&mut self, op: Op) {
        if let Some(a) = self.answer.take() {
            if a.value == "Error" {
                self.answer = Some(a);
                return;
            }
            self.tokens = vec![Token::Num(a.value)];
        }
        self.tokens.push(Token::Op(op));
    }

    fn equals(&mut self) {
        if self.answer.is_some() || self.tokens.is_empty() {
            return;
        }
        let opened = self.tokens.iter().filter(|t| matches!(t, Token::Open)).count();
        let closed = self.tokens.iter().filter(|t| matches!(t, Token::Close)).count();
        for _ in closed..opened {
            self.tokens.push(Token::Close);
        }
        let answer = Answer {
            expression: show_expression(&self.tokens),
            value: Parser::evaluate(&self.tokens).map_or_else(|| "Error".to_string(), format_number),
        };
        // A lone number is its own result, not worth an entry.
        if answer.value != "Error" && self.tokens.len() > 1 {
            self.history.insert(
                0,
                Entry {
                    id: self.next_id,
                    answer: answer.clone(),
                },
            );
            self.history.truncate(MAX_HISTORY);
            self.next_id += 1;
        }
        self.tokens.clear();
        self.answer = Some(answer);
    }

    fn back(&mut self) {
        if self.answer.is_some() {
            return;
        }
        match self.tokens.last_mut() {
            Some(Token::Num(n)) => {
                n.pop();
                if n.is_empty() || n == "-" {
                    self.tokens.pop();
                }
            }
            Some(_) => {
                self.tokens.pop();
            }
            None => {}
        }
    }

    /// The quiet line above the number: the last calculation, or nothing.
    fn last(&self) -> String {
        match self.history.first() {
            Some(e) => format!("{} = {}", e.answer.expression, e.answer.value),
            None => String::new(),
        }
    }

    /// Takes up a history entry: `expr` puts its expression back on the
    /// small line, to be changed and worked out again; `value` starts again
    /// from its result.
    fn recall(&mut self, id: &str, half: &str) -> Result<(), String> {
        let entry = self
            .history
            .iter()
            .find(|e| e.id.to_string() == id)
            .ok_or_else(|| format!("no history entry {id}"))?;
        match half {
            "expr" => {
                self.tokens = read_expression(&entry.answer.expression)
                    .ok_or_else(|| format!("history entry {id} cannot be read back"))?;
                self.answer = None;
            }
            "value" => {
                self.answer = Some(entry.answer.clone());
                self.tokens.clear();
            }
            _ => return Err(format!("a history entry has no half {half:?}")),
        }
        Ok(())
    }

    /// Do what the button with this key does.
    fn press(&mut self, action: &str) -> Result<(), String> {
        match action {
            "clear" => {
                self.tokens.clear();
                self.answer = None;
            }
            "clear-history" => self.history.clear(),
            "toggle-history" => self.history_open = !self.history_open,
            "back" => self.back(),
            "dot" => self.dot(),
            "open" => self.open(),
            "close" => self.close(),
            "add" => self.operator(Op::Add),
            "sub" => self.operator(Op::Sub),
            "mul" => self.operator(Op::Mul),
            "div" => self.operator(Op::Div),
            "eq" => self.equals(),
            "copy" => {
                clipboard::write_text(self.display()).map_err(|e| format!("cannot copy the result: {e:?}"))?;
            }
            _ => {
                if let Some((id, half)) = action.strip_prefix('h').and_then(|a| a.split_once('-')) {
                    return self.recall(id, half);
                }
                match action.strip_prefix('k') {
                    Some(d) if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => self.digit(d),
                    _ => return Err(format!("no action named {action:?}")),
                }
            }
        }
        Ok(())
    }
}

impl Page for Calculator {
    fn open(_page: PageId, state: Option<Vec<u8>>, _args: Option<Vec<u8>>) -> Self {
        state
            .as_deref()
            .and_then(Calculator::decode)
            .unwrap_or_else(Calculator::new)
    }

    fn event(&mut self, event: Event) -> Result<(), String> {
        match event {
            Event::Click { key } => self.press(&key),
            Event::Key(k) => match key_action(&k.key) {
                Some(action) => self.press(&action),
                None => Ok(()),
            },
            // tessoa keeps the bar within the page; a ratio beyond it would
            // make the saved state unreadable, so it is not taken.
            Event::Split { key, ratio } if key == "panes" => {
                if ratio > 0.0 && ratio < 1.0 {
                    self.ratio = ratio;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn render(&self) -> Node {
        let key = |key: &str, label: &str| btn(label).key(key).grow();
        let toggle = if self.history_open {
            icon_btn("menu", "Hide history").kind(ButtonKind::Normal)
        } else {
            icon_btn("menu", "Show history").kind(ButtonKind::Quiet)
        };
        let calculator = col([
            row([
                text(self.last())
                    .key("last")
                    .size(TextSize::Caption)
                    .tone(Tone::Dim)
                    .grow(),
                toggle.key("toggle-history"),
            ])
            .gap(Space::S)
            .align(Align::Center),
            text(self.expression())
                .key("expression")
                .size(TextSize::Caption)
                .tone(Tone::Dim)
                .align(Align::End),
            row([
                icon_btn("copy", "Copy").key("copy"),
                text(self.display())
                    .key("display")
                    .size(TextSize::Display)
                    .align(Align::End)
                    .grow(),
            ])
            .gap(Space::S)
            .align(Align::Center),
            row([key("clear", "C"), key("open", "("), key("close", ")"), key("div", "÷")]).gap(Space::S),
            row([key("k7", "7"), key("k8", "8"), key("k9", "9"), key("mul", "×")]).gap(Space::S),
            row([key("k4", "4"), key("k5", "5"), key("k6", "6"), key("sub", "−")]).gap(Space::S),
            row([key("k1", "1"), key("k2", "2"), key("k3", "3"), key("add", "+")]).gap(Space::S),
            row([
                key("k0", "0"),
                key("dot", "."),
                key("back", "⌫"),
                key("eq", "=").kind(ButtonKind::Primary),
            ])
            .gap(Space::S),
        ])
        .pad(Space::M)
        .gap(Space::S);
        if !self.history_open {
            return calculator;
        }
        let entries: Vec<Node> = if self.history.is_empty() {
            vec![
                text("No calculations yet")
                    .key("history-empty")
                    .tone(Tone::Dim)
                    .align(Align::Center),
            ]
        } else {
            self.history
                .iter()
                .map(|e| {
                    row([
                        btn(&e.answer.expression)
                            .key(format!("h{}-expr", e.id))
                            .kind(ButtonKind::Quiet)
                            .tooltip("Use this expression"),
                        btn(format!("= {}", e.answer.value))
                            .key(format!("h{}-value", e.id))
                            .kind(ButtonKind::Quiet)
                            .tooltip("Use this result"),
                    ])
                    .key(format!("h{}", e.id))
                    .gap(Space::S)
                    .justify(Justify::Between)
                    .align(Align::Center)
                })
                .collect()
        };
        let history = col([
            row([
                text("History").size(TextSize::Caption).tone(Tone::Dim).grow(),
                btn("Clear history")
                    .key("clear-history")
                    .kind(ButtonKind::Quiet)
                    .disabled(self.history.is_empty()),
            ])
            .gap(Space::S)
            .align(Align::Center),
            col(entries).key("history").gap(Space::Xs).scroll().grow(),
        ])
        .pad(Space::M)
        .gap(Space::S);
        col([split(self.ratio, calculator, history).key("panes").grow()])
    }

    fn keys(&self) -> Vec<String> {
        KEYS.iter().map(|k| k.to_string()).collect()
    }

    fn save(&self) -> Option<Vec<u8>> {
        Some(self.encode().into_bytes())
    }
}

tessoa_plugin_sdk::export_pages!(Calculator => "calculator");

#[cfg(test)]
mod tests {
    use super::*;

    fn run(actions: &[&str]) -> Calculator {
        let mut c = Calculator::new();
        for a in actions {
            c.press(a).unwrap();
        }
        c
    }

    #[test]
    fn numbers_are_rounded_to_ten_decimals() {
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(1.0 / 3.0), "0.3333333333");
        assert_eq!(format_number(2.0 / 3.0), "0.6666666667");
        assert_eq!(format_number(-2.5), "-2.5");
        assert_eq!(format_number(-1e-12), "0");
        assert_eq!(format_number(1e14), "100000000000000");
        assert_eq!(format_number(1e15), "Error");
        assert_eq!(format_number(f64::INFINITY), "Error");
        assert_eq!(format_number(f64::NAN), "Error");
    }

    #[test]
    fn expressions_follow_precedence_and_brackets() {
        let c = run(&["k2", "add", "k3", "mul", "k4", "eq"]);
        assert_eq!((c.display(), c.expression().as_str()), ("14", "2 + 3 × 4 ="));
        assert_eq!(
            run(&["open", "k2", "add", "k3", "close", "mul", "k4", "eq"]).display(),
            "20"
        );
        assert_eq!(
            run(&["k8", "sub", "k2", "sub", "k1", "eq"]).display(),
            "5",
            "left to right"
        );
        assert_eq!(run(&["k2", "mul", "sub", "k3", "eq"]).display(), "-6", "a minus sign");
        let c = run(&["k2", "mul", "open", "k3", "add", "open", "k1", "eq"]);
        assert_eq!(
            (c.display(), c.expression().as_str()),
            ("8", "2 × (3 + (1)) ="),
            "open brackets are closed at ="
        );
        let c = run(&["k2", "open", "k3", "close", "k4"]);
        assert_eq!(c.expression(), "2 × (3) × 4", "a bracket next to a number multiplies");
    }

    #[test]
    fn what_cannot_be_worked_out_is_an_error() {
        for actions in [
            &["k2", "close", "eq"][..],
            &["k2", "add", "eq"],
            &["mul", "k2", "eq"],
            &["open", "close", "eq"],
            &["k1", "div", "k0", "eq"],
        ] {
            assert_eq!(run(actions).display(), "Error", "{actions:?}");
        }
        assert_eq!(
            run(&["k1", "div", "k0", "eq", "k5"]).display(),
            "5",
            "a digit starts over"
        );
        assert_eq!(run(&["k1", "div", "k0", "eq", "add"]).display(), "Error");
        assert!(run(&["k1", "div", "k0", "eq"]).history.is_empty());
    }

    #[test]
    fn typing_and_editing() {
        assert_eq!(run(&["dot", "k5", "dot", "k1"]).display(), "0.51");
        assert_eq!(run(&["k1", "k2", "back", "back", "back"]).display(), "0");
        assert_eq!(run(&["k1", "add", "open", "back", "back"]).expression(), "1");
        let c = run(&["k6", "eq", "add", "k1", "eq"]);
        assert_eq!(
            (c.display(), c.history.len()),
            ("7", 1),
            "an operator goes on from the result"
        );
    }

    #[test]
    fn the_history_keeps_results_and_gives_them_back() {
        let mut c = run(&["k2", "add", "k2", "eq", "k3", "mul", "k3", "eq"]);
        let shown: Vec<_> = c.history.iter().map(|e| (e.id, e.answer.value.as_str())).collect();
        assert_eq!(shown, [(2, "9"), (1, "4")], "newest first");
        assert_eq!(c.last(), "3 × 3 = 9", "the last calculation shows above the number");
        c.press("k7").unwrap();
        c.press("h1-value").unwrap();
        assert_eq!((c.display(), c.expression().as_str()), ("4", "2 + 2 ="));
        c.press("add").unwrap();
        c.press("k1").unwrap();
        c.press("eq").unwrap();
        assert_eq!(c.display(), "5");
        assert!(c.press("h99-value").is_err());
        assert!(c.press("h1-both").is_err());
        c.press("clear-history").unwrap();
        assert!(c.history.is_empty());
        assert_eq!(c.last(), "");

        let mut c = Calculator::new();
        for _ in 0..MAX_HISTORY + 5 {
            for a in ["k1", "add", "k1", "eq"] {
                c.press(a).unwrap();
            }
        }
        assert_eq!(c.history.len(), MAX_HISTORY);
        assert_eq!(c.history[0].id as usize, MAX_HISTORY + 5);
    }

    #[test]
    fn an_expression_from_the_history_can_be_changed() {
        let mut c = run(&["k2", "add", "k3", "mul", "k4", "eq", "k9"]);
        c.press("h1-expr").unwrap();
        assert_eq!((c.display(), c.expression().as_str()), ("4", "2 + 3 × 4"));
        c.press("back").unwrap();
        c.press("k5").unwrap();
        c.press("eq").unwrap();
        assert_eq!((c.display(), c.history.len()), ("17", 2));

        // Every expression the page writes reads back to the same tokens.
        for actions in [
            &["k2", "mul", "sub", "k3", "eq"][..],
            &["k2", "mul", "open", "k3", "add", "open", "k1", "eq"],
            &["k1", "sub", "k7", "eq", "mul", "dot", "k5", "eq"],
            &["open", "k2", "close", "open", "k3", "close", "eq"],
        ] {
            let c = run(actions);
            let e = &c.history[0].answer.expression;
            let tokens = read_expression(e).unwrap();
            assert_eq!(&show_expression(&tokens), e, "{actions:?}");
        }
        assert_eq!(
            read_expression("-6 × 2"),
            Some(vec![
                Token::Num("-6".into()),
                Token::Op(Op::Mul),
                Token::Num("2".into())
            ])
        );
        assert_eq!(read_expression("2 ^ 3"), None);
    }

    #[test]
    fn the_history_opens_and_closes() {
        let mut c = Calculator::new();
        assert!(!c.history_open, "closed at first");
        c.event(Event::Key(KeyPress {
            key: "Ctrl+h".into(),
            ctrl: true,
            shift: false,
            alt: false,
            meta: false,
        }))
        .unwrap();
        assert!(c.history_open);
        c.press("toggle-history").unwrap();
        assert!(!c.history_open);
    }

    #[test]
    fn the_state_survives_a_round_trip() {
        let c = run(&["k1", "add", "k1", "eq", "open", "k1", "dot", "k5", "mul"]);
        assert_eq!(Calculator::decode(c.encode().as_bytes()), Some(c));
        let c = run(&["k4", "sub", "k5", "eq", "sub"]);
        assert_eq!(Calculator::decode(c.encode().as_bytes()), Some(c));
        let mut c = run(&["k2", "add", "k2", "eq", "toggle-history"]);
        c.ratio = 0.7;
        assert_eq!(Calculator::decode(c.encode().as_bytes()), Some(c));
        assert_eq!(Calculator::decode(b"garbage"), None);
        assert_eq!(
            Calculator::decode(b"calculator 3\n\n\n1\n1 1.5"),
            None,
            "a ratio out of range"
        );
        // What the previous version saved: its history stays, closed.
        let old = Calculator::decode(b"calculator 2\n\n2 + 2\t4\n2\n1\t2 + 2\t4").unwrap();
        assert_eq!((old.history.len(), old.history_open, old.ratio), (1, false, RATIO));
        assert_eq!((old.display(), old.next_id), ("4", 2));
        // What the version without a history saved: display, operand,
        // operation, fresh.
        assert_eq!(Calculator::decode(b"42\n42\n+\n1"), None);
    }
}
