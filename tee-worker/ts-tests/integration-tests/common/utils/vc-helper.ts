import type { HexString } from '@polkadot/util/types';
import { decryptWithAes } from './crypto';

export async function handleVcEvents(
    aesKey: HexString,
    events: any[],
    method: 'VCIssued' | 'VCDisabled' | 'VCRevoked' | 'Failed'
): Promise<any> {
    const results: any = [];
    for (let k = 0; k < events.length; k++) {
        switch (method) {
            case 'VCIssued':
                results.push({
                    account: events[k].data.account.toHex(),
                    index: events[k].data.index.toHex(),
                    vc: decryptWithAes(aesKey, events[k].data.vc, 'utf-8'),
                });
                break;
            case 'VCDisabled':
                results.push(events[k].data.index.toHex());
                break;
            case 'VCRevoked':
                results.push(events[k].data.index.toHex());
                break;
            case 'Failed':
                results.push(events[k].data.detail.toHuman());
                break;
            default:
                break;
        }
    }
    return [...results];
}
