{-# LANGUAGE TypeFamilies,TypeApplications,NoTypeAbstractions #-}
class C a where
  type F @k a
main :: IO ()
main = pure ()
