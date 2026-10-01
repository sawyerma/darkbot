"""Pinned token-account profiles and Solana PDA derivation for background fee probes.

PDA/ATA derivation is never part of the decision path. Creation evidence is extracted
from the already received transaction logs, scoped to the actual invoked programs.
"""
from __future__ import annotations

import base64
import hashlib
from typing import Any

ASSOCIATED_TOKEN_PROGRAM = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
SYSTEM_PROGRAM = '11111111111111111111111111111111'


def _on_curve(encoded: bytes) -> bool:
    # Ed25519 compressed-Y decompression (Solana rejects decompressible PDA keys).
    p = 2**255 - 19
    y = int.from_bytes(encoded, 'little') & (2**255 - 1)
    y %= p
    d = -121665 * pow(121666, p - 2, p) % p
    denominator = (d * y * y + 1) % p
    if not denominator:
        return False
    x2 = (y * y - 1) * pow(denominator, p - 2, p) % p
    return x2 == 0 or pow(x2, (p - 1) // 2, p) == 1


def find_pda(program: bytes, *seeds: bytes) -> bytes:
    if len(program) != 32 or len(seeds) >= 16 or any(len(s) > 32 for s in seeds):
        raise ValueError('invalid PDA seed/program length')
    for bump in range(255, -1, -1):
        key = hashlib.sha256(b''.join(seeds) + bytes([bump]) + program + b'ProgramDerivedAddress').digest()
        if not _on_curve(key):
            return key
    raise ValueError('no PDA bump')


def account_profiles(cfg: Any) -> dict[str, dict[str, Any]]:
    policy = cfg.execution['token_account_policy']
    return {program: profile for program, profile in policy['profiles'].items()
            if program == cfg.protocol['spl_token_program'] or policy['token_2022_supported']}


def create_account_profiles(logs: list[str], protocol: dict[str, Any]) -> dict[int, str]:
    """Return CREATE log indexes with complete causal CreateV2 account-size evidence.

    Failed/truncated invocations, foreign-program log imitations, extra extension
    instructions, and unexpected account sizes cannot qualify a mint. No RPC.
    """
    pump, token = protocol['program_id'], protocol['token_2022_program']
    profile = protocol['token_2022_create_profile']
    stack: list[dict[str, Any]] = []
    result: dict[int, str] = {}
    for index, line in enumerate(logs):
        parts = line.split()
        if len(parts) >= 3 and parts[0] == 'Program' and parts[2] == 'invoke':
            stack.append({'program': parts[1], 'create': False, 'instructions': set(), 'sizes': [], 'events': [], 'bad': False})
            continue
        if len(parts) >= 3 and parts[0] == 'Program' and parts[2] in ('success', 'failed:'):
            if not stack or stack[-1]['program'] != parts[1]:
                return {}  # malformed program context cannot establish a profile
            frame = stack.pop()
            if parts[2] != 'success':
                for parent in stack:
                    parent['bad'] = True
            if frame['create'] and not frame['bad'] and parts[2] == 'success':
                seen = frame['instructions']
                if (set(profile['required_token_instructions']) <= seen <= set(profile['allowed_token_instructions'])
                        and frame['sizes'] and all(s == profile['account_size_bytes'] for s in frame['sizes'])
                        and len(frame['events']) == 1):
                    result[frame['events'][0]] = profile['profile_id']
            continue
        if not stack:
            continue
        if stack[-1]['program'] == pump and line == 'Program log: Instruction: ' + profile['instruction']:
            stack[-1]['create'] = True
        create = next((f for f in reversed(stack) if f['create']), None)
        if create is None:
            continue
        if stack[-1] is create and line.startswith('Program data: '):
            # Event discriminator; only the single CreateEvent qualifies this frame.
            try:
                disc = base64.b64decode(line[len('Program data: '):], validate=True)[:8]
            except ValueError:
                create['bad'] = True
                continue
            if disc == CREATE_EVENT_DISCRIMINATOR:
                create['events'].append(index)
        if stack[-1]['program'] == token:
            if line.startswith('Program log: '):
                instruction = line[len('Program log: '):].removeprefix('Instruction: ')
                # Token program emitted text is part of the pinned profile contract.
                create['instructions'].add(instruction)
            if line.startswith('Program return: ' + token + ' '):
                try:
                    value = base64.b64decode(parts[-1], validate=True)
                    if len(value) != 8:
                        raise ValueError('invalid size')
                    create['sizes'].append(int.from_bytes(value, 'little'))
                except ValueError:
                    create['bad'] = True
    return result if not stack else {}


CREATE_EVENT_DISCRIMINATOR = hashlib.sha256(b'event:CreateEvent').digest()[:8]
