{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type T :: forall k. k -> Type
newtype T @k a = T (Proxy (a :: k))
main :: IO ()
main = pure ()
