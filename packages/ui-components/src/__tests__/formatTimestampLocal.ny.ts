/**
 * Child script for formatTimestampSecondsAsLocal local-time coverage.
 * Run with TZ=America/New_York so Date local getters are non-UTC.
 *
 * Imports the production formatter (not a duplicate) via vite-node.
 */
import { formatTimestampSecondsAsLocal } from "../lib/services/time";

process.stdout.write(formatTimestampSecondsAsLocal(BigInt("1672531200"), true));
