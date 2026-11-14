{-# LANGUAGE FlexibleInstances, KindSignatures, TypeFamilies, UndecidableInstances #-}
module CacheBootMono where
import Data.Kind (Type)
class Universal a where universal :: a -> Int
instance Universal (a :: Type)
class Describe a where describe :: a -> Int
instance (b ~ [a], Show b) => Describe (a :: Type)
