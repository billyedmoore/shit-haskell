{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F (a :: k)
type instance forall a. F @k a = Maybe a
main :: IO ()
main = pure ()
