{-# LANGUAGE TypeFamilies #-}
module CacheBootFamilyConsumer where
import {-# SOURCE #-} CacheBootFamily
type instance Family Bool = Int
fromConsumer :: Family Bool
fromConsumer = 23
