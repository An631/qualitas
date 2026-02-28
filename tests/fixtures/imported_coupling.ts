// Expected: file_dependencies.importCount > 0, function detects API calls
// File-level DC: 5 external imports, high external_ratio
// Function-level DC: distinct API calls from path, fs, crypto

import axios from 'axios';
import fs from 'fs';
import path from 'path';
import { EventEmitter } from 'events';
import crypto from 'crypto';

export async function fetchAndSaveData(url: string, outputDir: string): Promise<object> {
  const filename = path.basename(url);
  const outputPath = path.join(outputDir, filename);

  const response = await axios.get(url);
  const data = JSON.stringify(response.data, null, 2);

  const hash = crypto.createHash('sha256').update(data).digest('hex');

  fs.mkdirSync(outputDir, { recursive: true });
  fs.writeFileSync(outputPath, data, 'utf8');
  fs.writeFileSync(outputPath + '.hash', hash, 'utf8');

  const emitter = new EventEmitter();
  emitter.emit('saved', { path: outputPath, hash });

  return { path: outputPath, hash, size: data.length };
}
