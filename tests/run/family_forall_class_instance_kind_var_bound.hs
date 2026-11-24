{-# LANGUAGE PolyKinds, TypeFamilies, StandaloneKindSignatures, TypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
class C (a :: k)
instance forall k (a :: k). C @Type (Proxy a)
main :: IO ()
main = pure ()
