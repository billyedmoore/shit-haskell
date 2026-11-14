{-# LANGUAGE FlexibleContexts #-}
module CacheBootGndConsumer where
import {-# SOURCE #-} CacheBootGnd
import CacheBootRep
callN :: Multi b (Rep a b) => b -> N a b -> Int
callN = multi
