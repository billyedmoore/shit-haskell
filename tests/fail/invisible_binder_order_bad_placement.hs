{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,StandaloneKindSignatures #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type T :: forall k. k -> Type
data T a @k = T
main :: IO ()
main = pure ()
