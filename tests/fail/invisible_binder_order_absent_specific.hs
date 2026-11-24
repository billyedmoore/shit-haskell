{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,StandaloneKindSignatures #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
type T :: Type -> Type
data T @k a = T
main :: IO ()
main = pure ()
