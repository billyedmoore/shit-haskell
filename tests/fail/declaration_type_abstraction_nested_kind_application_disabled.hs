{-# LANGUAGE PolyKinds,TypeAbstractions,NoTypeApplications,StandaloneKindSignatures,DataKinds #-}
import Data.Kind (Type)
data K (a :: k)
type T :: forall k. K Bool -> Type
data T @k (a :: K @Type Bool)
main :: IO ()
main = pure ()
