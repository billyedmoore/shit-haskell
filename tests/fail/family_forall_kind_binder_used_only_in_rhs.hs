{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F (a :: k)
type instance forall k a. F a = Proxy @k a
main :: IO ()
main = pure ()
