{-# LANGUAGE GeneralizedNewtypeDeriving, DerivingStrategies, StandaloneDeriving, UndecidableInstances, MultiParamTypeClasses, FlexibleContexts, NoPolyKinds #-}
module CacheBootGnd where
import CacheBootRep
newtype N a b = N (Rep a b)
deriving newtype instance Multi b (Rep a b) => Multi b (N a b)
makeN :: Int -> N a b
makeN n = N (Rep n)
