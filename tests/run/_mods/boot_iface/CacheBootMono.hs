{-# LANGUAGE FlexibleInstances, TypeFamilies, UndecidableInstances #-}
module CacheBootMono where
class Universal a where universal :: a -> Int
instance Universal a where universal _ = 73
class Describe a where describe :: a -> Int
instance (b ~ [a], Show b) => Describe a where describe _ = 81
