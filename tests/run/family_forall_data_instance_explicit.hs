{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
data family F a
data instance forall a. F [a] = F a
main :: IO ()
main = pure ()
