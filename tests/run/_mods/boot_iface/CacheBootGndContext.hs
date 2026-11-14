{-# LANGUAGE GeneralizedNewtypeDeriving, DerivingStrategies #-}
module CacheBootGndContext where
newtype N a = N [a] deriving newtype Eq
makeN :: a -> N a
makeN x = N [x]
