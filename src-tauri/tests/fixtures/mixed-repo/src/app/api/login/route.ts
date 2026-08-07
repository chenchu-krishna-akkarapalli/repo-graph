import { db } from '../../../lib/db';

export async function POST() {
  return db.query('select 1');
}
