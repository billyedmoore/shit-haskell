{-# LANGUAGE PolyKinds,TypeAbstractions,TypeApplications,StandaloneKindSignatures,DataKinds #-}
import Data.Kind (Type)
data K (a :: k)
type T :: forall k. K @Type Bool -> Type
data T @k (a :: K @Type Bool)
main :: IO ()
main = pure ()
