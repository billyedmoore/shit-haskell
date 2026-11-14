{-# LANGUAGE MultiParamTypeClasses, FlexibleInstances, FlexibleContexts, UndecidableInstances, RoleAnnotations, NoPolyKinds #-}
module CacheBootGnd where
import CacheBootRep
data N a b
type role N phantom phantom
instance Multi b (Rep a b) => Multi b (N a b)
makeN :: Int -> N a b
