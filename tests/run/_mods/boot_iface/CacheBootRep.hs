{-# LANGUAGE MultiParamTypeClasses, FlexibleInstances, NoPolyKinds #-}
module CacheBootRep where
data Rep a b = Rep Int
class Multi b t where multi :: b -> t -> Int
instance Multi b (Rep a b) where multi _ (Rep n) = n
