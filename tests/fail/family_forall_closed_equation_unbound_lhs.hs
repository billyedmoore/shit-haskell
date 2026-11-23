{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F a where
  forall a. F [(a,b)] = Maybe a
main :: IO ()
main = pure ()
