{-# LANGUAGE PolyKinds,TypeFamilies,CUSKs,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type)
type family F @k (a :: k) :: Type
main :: IO ()
main = pure ()
