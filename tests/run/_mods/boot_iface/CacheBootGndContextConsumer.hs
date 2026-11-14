module CacheBootGndContextConsumer where
import {-# SOURCE #-} CacheBootGndContext
sameN :: Eq a => N a -> N a -> Bool
sameN = (==)
