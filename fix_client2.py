# Read the file
with open(r'src\solana\client.rs', 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find and fix the problematic lines
output_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    
    # Fix get_latest_blockhash
    if '            self.primary_client' in line and i + 2 < len(lines):
        next_line = lines[i+1]
        next_next = lines[i+2]
        
        if '.get_latest_blockhash()' in next_line and '.or_else(|_| self.try_backup(|client| client.get_latest_blockhash()))' in next_next:
            output_lines.append(line)
            output_lines.append('                .get_latest_blockhash()\n')
            output_lines.append('                .map_err(|e: solana_client::client_error::ClientError| e.into())\n')
            i += 4  # Skip the map_err line too
            continue
    
    # Fix get_account
    if '.get_account(pubkey)' in line and i + 1 < len(lines):
        next_line = lines[i+1]
        if '.or_else(|_| self.try_backup(|client| client.get_account(pubkey)))' in next_line:
            output_lines.append(line)
            output_lines.append('                .map_err(|e: solana_client::client_error::ClientError| e.into())\n')
            i += 3  # Skip or_else and map_err
            continue
    
    # Fix get_multiple_accounts
    if '.get_multiple_accounts(pubkeys)' in line and i + 1 < len(lines):
        next_line = lines[i+1]
        if '.or_else(|_| self.try_backup(|client| client.get_multiple_accounts(pubkeys)))' in next_line:
            output_lines.append(line)
            output_lines.append('                .map_err(|e: solana_client::client_error::ClientError| e.into())\n')
            i += 3  # Skip or_else and map_err
            continue
    
    # Fix get_balance
    if '.get_balance(pubkey)' in line and i + 1 < len(lines):
        next_line = lines[i+1]
        if '.or_else(|_| self.try_backup(|client| client.get_balance(pubkey)))' in next_line:
            output_lines.append(line)
            output_lines.append('                .map_err(|e: solana_client::client_error::ClientError| e.into())\n')
            i += 3  # Skip or_else and map_err
            continue
    
    output_lines.append(line)
    i += 1

# Write back
with open(r'src\solana\client.rs', 'w', encoding='utf-8') as f:
    f.writelines(output_lines)

print('Fixed client.rs - removed backup calls and added type annotations')
