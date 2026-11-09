class Describable a where
  describe :: a -> String
  describe _ = "thing"
  name' :: a -> String

data Dog = Dog
data Cat = Cat { catName :: String }

instance Describable Dog where
  name' _ = "dog"

instance Describable Cat where
  describe c = "cat named " ++ catName c
  name' = catName

instance Describable a => Describable (Maybe a) where
  describe Nothing = "nothing"
  describe (Just x) = "just " ++ describe x
  name' = maybe "none" name'

instance Describable a => Describable [a] where
  describe xs = unwords (map describe xs)
  name' xs = concatMap name' xs

main :: IO ()
main = do
  putStrLn (describe Dog ++ " / " ++ name' Dog)
  putStrLn (describe (Cat "Tom"))
  putStrLn (describe (Just (Cat "Felix")))
  putStrLn (describe (Nothing :: Maybe Dog))
  putStrLn (describe [Cat "a", Cat "b"])
  putStrLn (name' [Cat "x", Cat "y"])
