{-# LANGUAGE TypeFamilies,TypeAbstractions,NoTypeApplications #-}
class C a where
  type F @k a
main :: IO ()
main = pure ()
