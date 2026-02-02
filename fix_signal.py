import re

# Read the file
with open(r'src\copy_trader\signal.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Add fmt import if not present
if 'use std::fmt;' not in content:
    content = content.replace('use std::collections::HashSet;', 'use std::collections::HashSet;\nuse std::fmt;')

# Add Display implementation after TradeAction enum
if 'impl fmt::Display for TradeAction' not in content:
    # Find the TradeAction enum and add Display after it
    pattern = r'(pub enum TradeAction \{\s+Buy,\s+Sell,\s+\})'
    replacement = r'\1\n\nimpl fmt::Display for TradeAction {\n    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {\n        match self {\n            TradeAction::Buy => write!(f, "Buy"),\n            TradeAction::Sell => write!(f, "Sell"),\n        }\n    }\n}'
    content = re.sub(pattern, replacement, content, flags=re.MULTILINE | re.DOTALL)

# Write back
with open(r'src\copy_trader\signal.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('Added Display trait to TradeAction')
