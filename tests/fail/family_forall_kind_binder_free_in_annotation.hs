{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, ExplicitForAll, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type family F (a :: k)
type instance forall (a :: k). F a = Proxy a
main :: IO ()
main = pure ()
