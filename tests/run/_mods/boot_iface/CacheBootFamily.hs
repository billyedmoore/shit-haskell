{-# LANGUAGE TypeFamilies #-}
module CacheBootFamily where
import CacheBootFamilyConsumer (fromConsumer)
type family Family a
answer :: Int
answer = fromConsumer
