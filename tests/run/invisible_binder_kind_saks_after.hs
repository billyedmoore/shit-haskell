{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,StandaloneKindSignatures #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
data T @k (a :: k) = T
type T :: forall k. k -> Type
main :: IO ()
main = pure ()
