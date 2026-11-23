{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
type T :: forall k. k -> Type
data T @(_ :: Type) a = T
main :: IO ()
main = pure ()
