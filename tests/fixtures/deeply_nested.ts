// Expected: low score (grade D or F), HIGH_COGNITIVE_FLOW flag, DEEP_NESTING flag

export function processOrders(orders: any[], config: any, logger: any, db: any, cache: any, validator: any) {
  const results: any[] = [];
  for (const order of orders) {
    if (order.status === 'pending') {
      if (order.items && order.items.length > 0) {
        for (const item of order.items) {
          if (item.quantity > 0) {
            if (item.price !== undefined && item.price !== null) {
              try {
                if (validator.isValid(item)) {
                  if (cache.has(item.id)) {
                    const cached = cache.get(item.id);
                    if (cached.price !== item.price) {
                      cache.invalidate(item.id);
                      db.update(item);
                      logger.log('cache invalidated for item ' + item.id);
                    }
                  } else {
                    db.insert(item);
                    cache.set(item.id, item);
                  }
                  results.push({ order: order.id, item: item.id, status: 'processed' });
                } else {
                  logger.warn('invalid item: ' + item.id);
                  results.push({ order: order.id, item: item.id, status: 'invalid' });
                }
              } catch (err: any) {
                logger.error('failed to process item: ' + err.message);
                results.push({ order: order.id, item: item.id, status: 'error' });
              }
            }
          }
        }
      }
    } else if (order.status === 'cancelled') {
      for (const item of order.items ?? []) {
        if (cache.has(item.id)) {
          cache.invalidate(item.id);
        }
      }
    } else if (order.status === 'completed') {
      logger.log('order already completed: ' + order.id);
    }
  }
  return results;
}
