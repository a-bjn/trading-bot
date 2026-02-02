import re

# Read the file
with open(r'src\copy_trader\follower.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Fix the before field - remove .as_ref()
content = content.replace('before: before.as_ref(),', 'before: before,')

# Fix the config.clone() issue - just pass config directly or reconstruct it
# The config struct doesn't implement Clone, so we need to pass it by reference or reconstruct
content = content.replace('config.clone()', 'config')

# Write back
with open(r'src\copy_trader\follower.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('Fixed follower.rs type issues')
