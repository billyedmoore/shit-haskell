{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, NoExplicitForAll #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F a
type instance forall a. F [a] = Maybe a
main :: IO ()
main = pure ()
