{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F a
type instance forall a b. F [a] = Maybe b
main :: IO ()
main = pure ()
